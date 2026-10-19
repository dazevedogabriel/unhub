//! Página "Macros": biblioteca e editor (tipos e ações como no editor do G HUB).

use std::collections::HashSet;

use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::dialogs::{Dialog, Pending, StepDialog, StepKind};
use super::theme::*;
use super::{App, Recording};
use crate::capture::Capture;
use crate::macros::{self, Macro, MacroType, Step};

fn tag_color(tag: &str) -> egui::Color32 {
    match tag {
        "DOWN" => ACCENT,
        "UP" => BLUE,
        "TEXT" => AMBER,
        "WAIT" => MUTED,
        "SCROLL" => PURPLE,
        "MEDIA" | "RUN" => PINK,
        _ => MUTED,
    }
}

impl App {
    fn current_macro(&mut self) -> Option<&mut Macro> {
        let n = self.cfg.macros.len();
        if n == 0 {
            return None;
        }
        self.sel_macro = self.sel_macro.min(n - 1);
        self.cfg.macros.get_mut(self.sel_macro)
    }

    fn fix_list(&mut self) {
        let Some(m) = self.cfg.macros.get(self.sel_macro) else { return };
        let lists = m.kind.lists();
        if !lists.contains(&self.sel_list) {
            self.sel_list = lists[0];
            self.sel_step = None;
        }
    }

    fn select_macro(&mut self, i: usize) {
        if i != self.sel_macro {
            self.stop_record();
        }
        self.sel_macro = i;
        self.sel_step = None;
        if let Some(m) = self.cfg.macros.get(i) {
            self.sel_list = m.kind.lists()[0];
        }
    }

    /// Insere após o passo selecionado (ou no fim), ou substitui o passo `editing`.
    pub fn insert_steps(&mut self, editing: Option<usize>, steps: Vec<Step>) {
        let list = self.sel_list;
        let sel = self.sel_step;
        let Some(m) = self.current_macro() else { return };
        let v = m.list_mut(list);
        let new_sel = if let Some(i) = editing {
            if i < v.len() && steps.len() == 1 {
                v[i] = steps.into_iter().next().unwrap();
            }
            Some(i)
        } else {
            let pos = sel.map(|s| (s + 1).min(v.len())).unwrap_or(v.len());
            let n = steps.len();
            v.splice(pos..pos, steps);
            if n > 0 { Some(pos + n - 1) } else { sel }
        };
        self.sel_step = new_sel;
        self.cfg_changed();
    }

    pub fn delete_macro(&mut self, id: &str) {
        // botões que usavam a macro voltam ao padrão
        if let (Some(d), Some(p)) = (self.device().cloned(), self.profile.as_mut()) {
            let mut changed = false;
            for i in 0..d.button_count {
                for g in [false, true] {
                    if self.cfg.macro_on_button(d.pid, p, i, g).map(|m| m.id == id).unwrap_or(false) {
                        p.set_button(i, App::default_binding(i, g), g);
                        changed = true;
                    }
                }
            }
            if changed {
                self.dirty = true;
            }
        }
        self.stop_record();
        self.cfg.macros.retain(|m| m.id != id);
        if let (Some(d), Some(p)) = (self.device().cloned(), self.profile.clone()) {
            self.cfg.prune(d.pid, &p);
        }
        self.sel_macro = self.sel_macro.min(self.cfg.macros.len().saturating_sub(1));
        self.sel_step = None;
        self.cfg_changed();
    }

    fn buttons_using(&self, id: &str) -> Vec<String> {
        let (Some(d), Some(p)) = (self.device(), &self.profile) else { return vec![] };
        let names = crate::util::button_names(d.pid, d.button_count);
        let mut out = vec![];
        for (i, b) in names.iter().enumerate() {
            for g in [false, true] {
                if self.cfg.macro_on_button(d.pid, p, i, g).map(|m| m.id == id).unwrap_or(false) {
                    out.push(format!("{} {}{}", b.code, b.name, if g { " (G-Shift)" } else { "" }));
                }
            }
        }
        out
    }

    fn import_ghub(&mut self) {
        let default = macros::ghub_settings_path();
        let path = if default.exists() {
            Some(default)
        } else {
            rfd::FileDialog::new().set_title("settings.db do G HUB").add_filter("settings.db", &["db"]).pick_file()
        };
        let Some(path) = path else { return };
        match macros::import_from_ghub(&path) {
            Err(e) => self.flash_err(format!("Não foi possível ler o G HUB: {e}")),
            Ok((found, warnings)) => {
                let existing: HashSet<String> = self.cfg.macros.iter().map(|m| m.name.clone()).collect();
                let total = found.len();
                let new: Vec<Macro> = found.into_iter().filter(|m| !existing.contains(&m.name)).collect();
                let added = new.len();
                if added > 0 {
                    self.sel_macro = self.cfg.macros.len();
                    self.cfg.macros.extend(new);
                    self.select_macro(self.sel_macro);
                    self.cfg_changed();
                }
                let mut body = format!("{added} macro(s) importada(s).");
                if total > added {
                    body += &format!(" {} já existiam e foram ignoradas.", total - added);
                }
                if total == 0 {
                    body = "Nenhuma macro criada por você foi encontrada no G HUB.".into();
                }
                let mut w: Vec<String> = warnings.into_iter().collect::<HashSet<_>>().into_iter().collect();
                w.sort();
                if !w.is_empty() {
                    body += "\n\nAvisos:\n";
                    body += &w.into_iter().take(8).collect::<Vec<_>>().join("\n");
                }
                self.dialog = Some(Dialog::Info { title: "Importar do G HUB".into(), body });
            }
        }
    }

    // --- gravação ------------------------------------------------------------------------------

    fn start_record(&mut self, ctx: &egui::Context) {
        self.stop_captures();
        match Capture::start(ctx.clone()) {
            Ok(cap) => self.rec = Some(Recording { cap, buf: vec![], down: HashSet::new(), last_ms: None }),
            Err(e) => self.flash_err(e.to_string()),
        }
    }

    pub fn stop_record(&mut self) {
        let Some(mut rec) = self.rec.take() else { return };
        // solta o que ficou pressionado quando a gravação parou
        let mut down: Vec<u16> = rec.down.drain().collect();
        down.sort();
        rec.buf.extend(down.into_iter().map(|c| Step::Key { code: c, down: false }));
        if !rec.buf.is_empty() {
            self.insert_steps(None, rec.buf);
        }
    }

    pub fn poll_record(&mut self) {
        let real = self.real_delays;
        let Some(rec) = &mut self.rec else { return };
        for e in rec.cap.drain() {
            if e.down && rec.down.contains(&e.hid) {
                continue; // auto-repetição
            }
            if e.down {
                rec.down.insert(e.hid);
            } else if !rec.down.remove(&e.hid) {
                continue; // soltou algo pressionado antes de gravar
            }
            if real {
                if let Some(last) = rec.last_ms {
                    let ms = e.time_ms.wrapping_sub(last);
                    if ms > 0 && ms < 600_000 {
                        rec.buf.push(Step::Delay { ms });
                    }
                }
            }
            rec.last_ms = Some(e.time_ms);
            rec.buf.push(Step::Key { code: e.hid, down: e.down });
        }
    }

    // --- página --------------------------------------------------------------------------------

    pub fn macros_page(&mut self, ui: &mut Ui, area: Rect) {
        self.fix_list();
        let lib = Rect::from_min_size(area.min, Vec2::new(240.0, area.height()));
        let ed = Rect::from_min_max(Pos2::new(lib.right() + 20.0, area.top()), area.max);
        self.library(ui, lib);
        paint_card(ui, ed);
        if self.cfg.macros.is_empty() {
            let c = ed.center();
            ui.painter().text(Pos2::new(c.x, c.y - 12.0), Align2::CENTER_CENTER, "Nenhuma macro ainda", fm(17.0), TEXT);
            ui.painter().text(Pos2::new(c.x, c.y + 14.0), Align2::CENTER_CENTER,
                "Crie uma em “+ Nova” ou importe as suas do G HUB.", f(13.0), MUTED);
            return;
        }
        self.editor(ui, ed);
    }

    fn library(&mut self, ui: &mut Ui, r: Rect) {
        paint_card(ui, r);
        let head = Rect::from_min_size(r.min, Vec2::new(r.width(), 46.0));
        ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, BORDER));
        paint_caps(ui, Pos2::new(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, "BIBLIOTECA", DIM);
        let nr = Rect::from_min_size(Pos2::new(head.right() - 66.0, head.top() + 10.0), Vec2::new(54.0, 26.0));
        let nresp = ui.interact(nr, ui.scope_id().with("newmacro"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().text(Pos2::new(nr.right(), nr.center().y), Align2::RIGHT_CENTER, "+ Nova", fb(12.0),
            if nresp.hovered() { ACCENT_HI } else { ACCENT });
        if nresp.clicked() {
            let name = self.cfg.unique_name("Nova macro");
            self.cfg.macros.push(Macro::new(&name, MacroType::Simple));
            self.select_macro(self.cfg.macros.len() - 1);
            self.cfg_changed();
        }

        let foot_h = 104.0;
        let foot = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - foot_h), r.max);
        ui.painter().line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, BORDER));
        let list = Rect::from_min_max(Pos2::new(r.left() + 6.0, head.bottom() + 6.0), Pos2::new(r.right() - 6.0, foot.top() - 4.0));
        let mut c = child(ui, list);
        let mut clicked = None;
        egui::ScrollArea::vertical().id_salt("macro-lib").auto_shrink([false, false]).show(&mut c, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, m) in self.cfg.macros.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(Vec2::new(list.width(), 40.0), Sense::click());
                let sel = i == self.sel_macro;
                if sel {
                    ui.painter().rect_filled(row, 8, CHIP);
                } else if resp.hovered() {
                    ui.painter().rect_filled(row, 8, hex(0x1E2126));
                }
                let n = m.step_count().to_string();
                let nw = text_size(ui, &n, mono(11.0)).x;
                paint_text(ui, Pos2::new(row.left() + 12.0, row.center().y), Align2::LEFT_CENTER, &m.name, fm(14.0),
                    if sel { TEXT } else { MUTED }, row.width() - 36.0 - nw);
                ui.painter().text(Pos2::new(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, n, mono(11.0), DIM);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clicked = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.select_macro(i);
        }

        let f = foot.shrink(12.0);
        let half = (f.width() - 6.0) / 2.0;
        let has = !self.cfg.macros.is_empty();
        let mut c1 = child(ui, Rect::from_min_size(f.min, Vec2::new(half, 34.0)));
        if !has {
            c1.disable();
        }
        if button_sized(&mut c1, "Duplicar", Btn::Outline, Some(half), 12.0).clicked() {
            if let Some(m) = self.cfg.macros.get(self.sel_macro) {
                let d = m.duplicate();
                self.cfg.macros.push(d);
                self.select_macro(self.cfg.macros.len() - 1);
                self.cfg_changed();
            }
        }
        let mut c2 = child(ui, Rect::from_min_size(Pos2::new(f.left() + half + 6.0, f.top()), Vec2::new(half, 34.0)));
        if !has {
            c2.disable();
        }
        if button_sized(&mut c2, "Excluir", Btn::Danger, Some(half), 12.0).clicked() {
            if let Some(m) = self.cfg.macros.get(self.sel_macro) {
                let users = self.buttons_using(&m.id);
                let mut body = format!("Excluir a macro “{}”?", m.name);
                if !users.is_empty() {
                    body += &format!("\n\nEla está atribuída a: {}. Esses botões voltarão ao padrão (aplique para gravar).", users.join(", "));
                }
                self.dialog = Some(Dialog::confirm("Excluir macro", &body, "Excluir", true, Pending::DeleteMacro(m.id.clone())));
            }
        }
        let mut c3 = child(ui, Rect::from_min_size(Pos2::new(f.left(), f.top() + 40.0), Vec2::new(f.width(), 34.0)));
        if button_sized(&mut c3, "Importar do G HUB", Btn::Dashed, Some(f.width()), 12.0).clicked() {
            self.import_ghub();
        }
    }

    fn editor(&mut self, ui: &mut Ui, r: Rect) {
        let ctx = ui.ctx().clone();
        let recording = self.rec.is_some();
        let rec_count = self.rec.as_ref().map(|r| r.buf.len()).unwrap_or(0);
        let list_name = self.sel_list;
        let sel_step = self.sel_step;
        let real_delays = self.real_delays;
        let started = self.started;
        let dialog_open = self.dialog.is_some();
        let mut sel_out = self.sel_step;
        let Some(m) = self.current_macro() else { return };
        let mut changed = false;

        // --- propriedades ------------------------------------------------------------------
        let top = Rect::from_min_size(r.min, Vec2::new(r.width(), 92.0));
        ui.painter().line_segment([top.left_bottom(), top.right_bottom()], Stroke::new(1.0, BORDER));
        let t = top.shrink(18.0);
        let (type_w, delay_w, gap) = (260.0, 200.0, 14.0);
        let name_w = t.width() - type_w - delay_w - 2.0 * gap;
        let mut x = t.left();
        paint_caps(ui, Pos2::new(x, t.top()), Align2::LEFT_TOP, "NOME", DIM);
        {
            let mut c = child(ui, Rect::from_min_size(Pos2::new(x, t.top() + 18.0), Vec2::new(name_w, 36.0)));
            let resp = c.add_sized(Vec2::new(name_w, 36.0),
                egui::TextEdit::singleline(&mut m.name).font(fm(14.0)).frame(input_frame()).text_color(TEXT));
            if resp.changed() {
                changed = true;
            }
            if resp.lost_focus() && m.name.trim().is_empty() {
                m.name = "Macro".into();
            }
        }
        x += name_w + gap;
        paint_caps(ui, Pos2::new(x, t.top()), Align2::LEFT_TOP, "TIPO", DIM);
        {
            let mut c = child(ui, Rect::from_min_size(Pos2::new(x, t.top() + 18.0), Vec2::new(type_w, 36.0)));
            let mut kind = m.kind;
            c.spacing_mut().button_padding = Vec2::new(10.0, 9.0);
            egui::ComboBox::from_id_salt("macro-type")
                .width(type_w - 22.0)
                .selected_text(egui::RichText::new(kind.label()).font(fm(14.0)).color(TEXT))
                .show_ui(&mut c, |ui| {
                    for k in MacroType::ALL {
                        ui.selectable_value(&mut kind, k, egui::RichText::new(k.label()).font(f(14.0)));
                    }
                });
            if kind != m.kind {
                m.change_type(kind);
                changed = true;
            }
        }
        x += type_w + gap;
        {
            let tr = Rect::from_min_size(Pos2::new(x, t.top() - 1.0), Vec2::new(delay_w, 16.0));
            let resp = ui.interact(tr, ui.scope_id().with("delaytoggle"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            paint_toggle(ui, Rect::from_min_size(tr.min, Vec2::new(26.0, 16.0)), m.use_default_delay, resp.hovered());
            paint_caps(ui, Pos2::new(tr.left() + 34.0, tr.center().y), Align2::LEFT_CENTER, "ATRASO PADRÃO", DIM);
            if resp.clicked() {
                m.use_default_delay = !m.use_default_delay;
                changed = true;
            }
            let mut c = child(ui, Rect::from_min_size(Pos2::new(x, t.top() + 18.0), Vec2::new(delay_w, 36.0)));
            if !m.use_default_delay {
                c.set_opacity(0.4);
            }
            if num_edit(&mut c, ("delay", &m.id), &mut m.default_delay, 0, 60_000, 1, delay_w, Some("ms")) {
                changed = true;
            }
        }

        // --- título do tipo + gravar ----------------------------------------------------------
        let bar = Rect::from_min_size(Pos2::new(r.left() + 18.0, top.bottom() + 14.0), Vec2::new(r.width() - 36.0, 40.0));
        let (title, desc) = m.kind.info();
        ui.painter().text(bar.left_top(), Align2::LEFT_TOP, title, fm(14.0), TEXT);
        let rec_label = if recording { format!("Gravando… {rec_count} · parar") } else { "Gravar teclas".to_string() };
        let rw = text_size(ui, &rec_label, fm(13.0)).x + 44.0;
        let rr = Rect::from_min_size(Pos2::new(bar.right() - rw, bar.top() + 2.0), Vec2::new(rw, 34.0));
        let rresp = ui.interact(rr, ui.scope_id().with("rec"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let (bg, fg, border) = if recording {
            (hex(0x2A1514), RED, hex(0x5A2622))
        } else {
            (if rresp.hovered() { HOVER } else { egui::Color32::TRANSPARENT }, TEXT2, BORDER2)
        };
        ui.painter().rect(rr, 8, bg, Stroke::new(1.0, border), egui::StrokeKind::Inside);
        let pulse = if recording {
            ctx.request_repaint();
            0.35 + 0.65 * (0.5 + 0.5 * (started.elapsed().as_secs_f32() * 4.0).cos())
        } else {
            1.0
        };
        ui.painter().circle_filled(Pos2::new(rr.left() + 18.0, rr.center().y), 4.0, alpha(REC, pulse));
        ui.painter().text(Pos2::new(rr.left() + 30.0, rr.center().y), Align2::LEFT_CENTER, &rec_label, fm(13.0), fg);

        // gravar atrasos reais (ao lado do botão de gravar)
        let mut toggle_real = false;
        let cb_w = text_size(ui, "atrasos reais", f(12.0)).x + 22.0;
        let mut right_edge = rr.left() - 14.0 - cb_w;
        {
            let mut c = child(ui, Rect::from_min_size(Pos2::new(right_edge, rr.top()), Vec2::new(cb_w, rr.height())));
            c.horizontal_centered(|ui| {
                if checkbox(ui, real_delays, "atrasos reais").on_hover_text("Grava também o tempo entre as teclas").clicked() {
                    toggle_real = true;
                }
            });
        }

        // lista ativa (só na sequência há três)
        let mut new_list = None;
        if m.kind == MacroType::Sequence {
            let lists = m.kind.lists();
            let labels: Vec<String> = lists.iter().map(|l| format!("{} · {}", l.label(), m.list(*l).len())).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let idx = lists.iter().position(|l| *l == list_name).unwrap_or(0);
            let segw: f32 = refs.iter().map(|s| text_size(ui, s, f(12.0)).x + 24.0).sum::<f32>() + 10.0;
            let sr = Rect::from_min_size(Pos2::new(right_edge - 14.0 - segw, bar.top() + 2.0), Vec2::new(segw, 34.0));
            right_edge = sr.left();
            let mut c = child(ui, sr);
            if let Some(i) = segmented(&mut c, &refs, idx, f(12.0), None, false) {
                new_list = Some(lists[i]);
            }
        }
        paint_text(ui, Pos2::new(bar.left(), bar.top() + 20.0), Align2::LEFT_TOP, desc, f(12.0), MUTED, right_edge - bar.left() - 16.0);

        // --- passos ---------------------------------------------------------------------------
        let foot = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 60.0), r.max);
        ui.painter().line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, BORDER));
        let list_r = Rect::from_min_max(Pos2::new(r.left() + 18.0, bar.bottom() + 8.0), Pos2::new(r.right() - 18.0, foot.top() - 6.0));
        let use_default = m.use_default_delay;
        let dd = m.default_delay;
        let steps = m.list_mut(list_name);
        let mut action: Option<(usize, i32)> = None; // (índice, -1 sobe / 1 desce / 0 remove / 2 seleciona / 3 edita)
        let mut c = child(ui, list_r);
        egui::ScrollArea::vertical().id_salt("macro-steps").auto_shrink([false, false]).show(&mut c, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if steps.is_empty() {
                let (er, _) = ui.allocate_exact_size(Vec2::new(list_r.width(), 96.0), Sense::hover());
                dashed_rect(ui, er.shrink(1.0), BORDER2);
                ui.painter().text(er.center(), Align2::CENTER_CENTER, "Nenhuma ação. Adicione abaixo ou grave.", f(13.0), DIM);
            }
            for (i, s) in steps.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(Vec2::new(list_r.width(), 44.0), Sense::click());
                let sel = sel_step == Some(i);
                let hov = resp.hovered();
                ui.painter().rect(row, 9, if sel { hex(0x1E2126) } else { hex(0x131518) },
                    Stroke::new(1.0, if sel { BORDER3 } else if hov { hex(0x33373E) } else { CHIP }), egui::StrokeKind::Inside);
                let cy = row.center().y;
                ui.painter().text(Pos2::new(row.left() + 12.0, cy), Align2::LEFT_CENTER, (i + 1).to_string(), mono(11.0), DIM);
                let (tag, label) = s.describe();
                let tf = monob(10.0);
                let tw = text_size(ui, tag, tf.clone()).x + 12.0;
                let tr = Rect::from_min_size(Pos2::new(row.left() + 52.0, cy - 10.0), Vec2::new(tw, 20.0));
                ui.painter().rect_filled(tr, 4, INPUT);
                ui.painter().text(tr.center(), Align2::CENTER_CENTER, tag, tf, tag_color(tag));
                let icons_w = 3.0 * 30.0;
                paint_text(ui, Pos2::new(row.left() + 124.0, cy), Align2::LEFT_CENTER, &label, f(14.0), TEXT,
                    row.width() - 124.0 - icons_w - 90.0);
                // atraso padrão que será inserido depois desta ação
                let next_is_delay = steps.get(i + 1).map(Step::is_delay).unwrap_or(false);
                if use_default && !s.is_delay() && !next_is_delay {
                    ui.painter().text(Pos2::new(row.right() - icons_w - 14.0, cy), Align2::RIGHT_CENTER,
                        format!("+{dd} ms"), mono(11.0), DIM);
                }
                let mut ix = row.right() - icons_w - 4.0;
                for (k, (glyph, danger)) in [("↑", false), ("↓", false), ("✕", true)].iter().enumerate() {
                    let ir = Rect::from_min_size(Pos2::new(ix, cy - 13.0), Vec2::splat(26.0));
                    let iresp = ui.interact(ir, ui.scope_id().with(("stepbtn", i, k)), Sense::click());
                    if iresp.hovered() {
                        ui.painter().rect_filled(ir, 6, if *danger { hex(0x2A1A1A) } else { SEL });
                    }
                    let col = match (iresp.hovered(), danger) {
                        (true, true) => RED,
                        (true, false) => TEXT,
                        _ => MUTED,
                    };
                    ui.painter().text(ir.center(), Align2::CENTER_CENTER, *glyph, f(13.0), col);
                    if iresp.clicked() {
                        action = Some((i, [-1, 1, 0][k]));
                    }
                    ix += 30.0;
                }
                if action.is_none() {
                    if resp.double_clicked() {
                        action = Some((i, 3));
                    } else if resp.clicked() {
                        action = Some((i, 2));
                    }
                }
            }
        });
        let mut edit_dialog = None;
        if let Some((i, a)) = action {
            match a {
                -1 | 1 => {
                    let j = i as i32 + a;
                    if j >= 0 && (j as usize) < steps.len() {
                        steps.swap(i, j as usize);
                        sel_out = Some(j as usize);
                        changed = true;
                    }
                }
                0 => {
                    steps.remove(i);
                    sel_out = if steps.is_empty() { None } else { Some(i.min(steps.len() - 1)) };
                    changed = true;
                }
                2 => sel_out = Some(i),
                _ => {
                    sel_out = Some(i);
                    edit_dialog = Some(StepDialog::edit(i, &steps[i]));
                }
            }
        }
        // Delete remove o passo selecionado
        if let Some(i) = sel_out {
            if i < steps.len() && !dialog_open && ctx.memory(|m| m.focused().is_none())
                && ctx.input_mut(|inp| inp.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                steps.remove(i);
                sel_out = if steps.is_empty() { None } else { Some(i.min(steps.len() - 1)) };
                changed = true;
            }
        }
        self.sel_step = sel_out;

        // --- adicionar ------------------------------------------------------------------------
        let fr = foot.shrink2(Vec2::new(18.0, 14.0));
        let mut add = None;
        {
            let mut c = child(ui, fr);
            c.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                caps(ui, "ADICIONAR");
                ui.add_space(6.0);
                for (k, label) in StepKind::ADD {
                    if add_chip(ui, label).clicked() {
                        add = Some(k);
                    }
                }
            });
        }

        if changed {
            self.cfg_changed();
        }
        if let Some(l) = new_list {
            self.sel_list = l;
            self.sel_step = None;
        }
        if toggle_real {
            self.real_delays = !self.real_delays;
        }
        if rresp.clicked() {
            if self.rec.is_some() {
                self.stop_record();
            } else {
                self.start_record(&ctx);
            }
        }
        if let Some(d) = edit_dialog {
            self.stop_captures();
            self.dialog = Some(Dialog::Step(d));
        }
        if let Some(k) = add {
            self.stop_captures();
            let dd = self.cfg.macros.get(self.sel_macro).map(|m| m.default_delay).unwrap_or(50);
            let mut d = StepDialog::new(k, dd);
            if k == StepKind::Key {
                d.cap = Capture::start(ctx.clone()).ok();
            }
            self.dialog = Some(Dialog::Step(d));
        }
    }
}
