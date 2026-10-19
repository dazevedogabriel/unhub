//! Página "Atalhos": desenho do mouse, lista de botões e painel de ações.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::dialogs::{BindingChoice, Dialog, Pending};
use super::theme::*;
use super::{mouse_art, App};
use crate::capture::{Capture, ComboBuilder};
use crate::keys;
use crate::onboard::Binding;
use crate::util;

pub struct ActionInfo {
    pub tag: String,
    pub tag_color: Color32,
    pub action: String,
    pub sub: String,
}

impl App {
    pub fn action_info(&self, button: usize, gshift: bool) -> ActionInfo {
        let (Some(d), Some(p)) = (self.device(), &self.profile) else {
            return ActionInfo { tag: "—".into(), tag_color: MUTED, action: String::new(), sub: String::new() };
        };
        if let Some(m) = self.cfg.macro_on_button(d.pid, p, button, gshift) {
            return ActionInfo { tag: tr!("buttons.tag_macro"), tag_color: AMBER, action: tr!("buttons.macro", name = m.name), sub: m.summary() };
        }
        let b = p.button(button, gshift);
        let (tag, tag_color, sub) = match b {
            Binding::Disabled => ("", MUTED, ""),
            Binding::Mouse(_) => ("buttons.tag_mouse", MUTED, ""),
            Binding::Key { .. } => ("buttons.tag_key", MINT, "buttons.sub_key"),
            Binding::Media(_) => ("buttons.tag_media", PINK, ""),
            Binding::Special(0x0B) => ("buttons.tag_shift", PURPLE, "buttons.sub_shift"),
            Binding::Special(0x03..=0x07) => ("buttons.tag_dpi", BLUE, ""),
            Binding::Special(_) => ("buttons.tag_fw", MUTED, "buttons.sub_fw"),
            Binding::Raw(_) => ("buttons.tag_raw", MUTED, "buttons.sub_raw"),
        };
        let tr_or_empty = |k: &str| if k.is_empty() { String::new() } else { tr!(k) };
        let tag = if tag.is_empty() { "—".to_string() } else { tr!(tag) };
        ActionInfo { tag, tag_color, action: b.describe(), sub: tr_or_empty(sub) }
    }

    /// Escolha vinda do painel de ações (pede confirmação ao tirar o clique esquerdo do G1).
    pub fn choose_binding(&mut self, choice: BindingChoice) {
        let (i, g) = (self.sel_button, self.gshift);
        let removes_left = i == 0 && !g && !matches!(choice, BindingChoice::Plain(Binding::Mouse(1)));
        let currently_left = self.profile.as_ref().map(|p| p.button(0, false) == Binding::Mouse(1)).unwrap_or(false);
        if removes_left && currently_left {
            self.dialog = Some(Dialog::confirm(
                &tr!("confirm.left_title"),
                &tr!("confirm.left_body"),
                &tr!("confirm.continue"),
                true,
                Pending::SetBinding(i, g, choice),
            ));
        } else {
            self.set_binding(i, g, choice);
        }
    }

    pub fn buttons_page(&mut self, ui: &mut Ui, area: Rect) {
        let Some(dev) = self.device().cloned() else { return };
        let gap = 20.0;
        let left = Rect::from_min_size(area.min, Vec2::new(250.0, area.height()));
        let right = Rect::from_min_max(Pos2::new(area.right() - 260.0, area.top()), area.max);
        let mid = Rect::from_min_max(Pos2::new(left.right() + gap, area.top()), Pos2::new(right.left() - gap, area.bottom()));
        let names = util::button_names(dev.pid, dev.button_count);

        // --- cartão com o desenho -------------------------------------------------------
        paint_card(ui, left);
        let l = left.shrink(18.0);
        {
            let mut c = child(ui, Rect::from_min_size(l.min, Vec2::new(l.width(), 40.0)));
            if let Some(i) = segmented(&mut c, &[&tr!("buttons.layer_normal"), &tr!("buttons.layer_gshift")], self.gshift as usize, fm(13.0), Some(l.width()), false) {
                self.gshift = i == 1;
                self.shortcut_cap = None;
            }
        }
        let hint = tr!(if self.gshift { "buttons.hint_gshift" } else { "buttons.hint" });
        let g = ui.painter().layout(hint, f(12.0), DIM, l.width());
        let hint_h = g.size().y;
        let gx = l.center().x - g.size().x / 2.0;
        ui.painter().galley(Pos2::new(gx, l.bottom() - hint_h), g, DIM);
        let art = Rect::from_min_max(Pos2::new(l.left(), l.top() + 52.0), Pos2::new(l.right(), l.bottom() - hint_h - 12.0));
        if dev.pid == 0xC08F {
            if let Some(i) = mouse_art::draw(ui, art, self.sel_button, dev.button_count) {
                self.select_button(i);
            }
        } else {
            ui.painter().text(art.center(), Align2::CENTER_CENTER, &dev.name, fm(16.0), MUTED);
        }

        // --- lista de botões --------------------------------------------------------------
        paint_card(ui, mid);
        let head = Rect::from_min_size(mid.min, Vec2::new(mid.width(), 38.0));
        ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, BORDER));
        let cols = [18.0, 62.0, 182.0];
        for (x, t) in cols.iter().zip(["buttons.col_id", "buttons.col_button", "buttons.col_action"]) {
            paint_caps(ui, Pos2::new(mid.left() + x, head.center().y), Align2::LEFT_CENTER, &tr!(t), DIM);
        }
        let body = Rect::from_min_max(Pos2::new(mid.left() + 1.0, head.bottom()), Pos2::new(mid.right() - 1.0, mid.bottom() - 1.0));
        let mut c = child(ui, body);
        egui::ScrollArea::vertical().id_salt("btn-list").auto_shrink([false, false]).show(&mut c, |ui| {
            for (i, b) in names.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(body.width(), 64.0), Sense::click());
                let sel = i == self.sel_button;
                let p = ui.painter();
                if sel {
                    p.rect_filled(r, 0, hex(0x1C1F24));
                    p.rect_filled(Rect::from_min_size(r.min, Vec2::new(2.0, r.height())), 0, ACCENT);
                } else if resp.hovered() {
                    p.rect_filled(r, 0, hex(0x1B1E23));
                }
                p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, ROWLINE));
                let cy = r.center().y;
                p.text(Pos2::new(r.left() + 17.0, cy), Align2::LEFT_CENTER, &b.code, monob(12.0), if sel { ACCENT } else { CODE });
                paint_text(ui, Pos2::new(r.left() + 61.0, cy), Align2::LEFT_CENTER, &b.name, f(14.0), TEXT2, 112.0);
                let info = self.action_info(i, self.gshift);
                let tag_font = mono(10.0);
                let tw = text_size(ui, &info.tag, tag_font.clone()).x + 12.0;
                let tr = Rect::from_min_size(Pos2::new(r.left() + 181.0, cy - 10.0), Vec2::new(tw, 20.0));
                ui.painter().rect_filled(tr, 4, INPUT);
                ui.painter().text(tr.center(), Align2::CENTER_CENTER, &info.tag, tag_font, info.tag_color);
                let ax = tr.right() + 10.0;
                let aw = r.right() - ax - 16.0;
                if info.sub.is_empty() {
                    paint_text(ui, Pos2::new(ax, cy), Align2::LEFT_CENTER, &info.action, fm(14.0), TEXT, aw);
                } else {
                    paint_text(ui, Pos2::new(ax, cy - 1.0), Align2::LEFT_BOTTOM, &info.action, fm(14.0), TEXT, aw);
                    paint_text(ui, Pos2::new(ax, cy + 2.0), Align2::LEFT_TOP, &info.sub, f(12.0), DIM, aw);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.select_button(i);
                }
            }
        });

        // --- painel de ações ----------------------------------------------------------------
        paint_card(ui, right);
        let rr = right.shrink(18.0);
        let sel = &names[self.sel_button.min(names.len() - 1)];
        let code = format!("{}{}", sel.code, if self.gshift { " · G-SHIFT" } else { "" });
        ui.painter().text(rr.left_top(), Align2::LEFT_TOP, code, mono(11.0), ACCENT);
        paint_text(ui, Pos2::new(rr.left(), rr.top() + 18.0), Align2::LEFT_TOP, &sel.name, fm(17.0), TEXT, rr.width());
        let reset = Rect::from_min_max(Pos2::new(rr.left(), rr.bottom() - 36.0), rr.max);
        let list = Rect::from_min_max(Pos2::new(right.left() + 4.0, rr.top() + 50.0), Pos2::new(right.right() - 4.0, reset.top() - 12.0));
        let mut c = child(ui, list);
        egui::ScrollArea::vertical().id_salt("btn-actions").auto_shrink([false, false]).show(&mut c, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 0, bottom: 0 }).show(ui, |ui| {
                ui.set_width(list.width() - 28.0);
                self.action_groups(ui);
            });
        });
        let mut c = child(ui, reset);
        let resp = button_sized(&mut c, &tr!("buttons.reset"), Btn::Outline, Some(reset.width()), 13.0);
        if resp.clicked() {
            let b = App::default_binding(self.sel_button, self.gshift);
            self.choose_binding(BindingChoice::Plain(b));
        }
    }

    fn select_button(&mut self, i: usize) {
        if i != self.sel_button {
            self.shortcut_cap = None;
        }
        self.sel_button = i;
    }

    fn action_groups(&mut self, ui: &mut Ui) {
        let (Some(dev), Some(p)) = (self.device().cloned(), self.profile.clone()) else { return };
        let (i, g) = (self.sel_button, self.gshift);
        let current = p.button(i, g);
        let current_macro = self.cfg.macro_on_button(dev.pid, &p, i, g).map(|m| m.id.clone());
        let is = |b: Binding| current_macro.is_none() && current == b;
        let mut pick: Option<BindingChoice> = None;

        let group = |ui: &mut Ui, key: &str| {
            ui.add_space(4.0);
            caps(ui, &tr!(key));
        };
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);

        group(ui, "buttons.group_mouse");
        ui.horizontal_wrapped(|ui| {
            for n in 1..=5u8 {
                if chip(ui, &crate::onboard::mouse_button_name(n), is(Binding::Mouse(n))).clicked() {
                    pick = Some(BindingChoice::Plain(Binding::Mouse(n)));
                }
            }
        });
        ui.add_space(10.0);
        group(ui, "buttons.group_dpi");
        ui.horizontal_wrapped(|ui| {
            for code in [0x05u8, 0x03, 0x04, 0x06, 0x07] {
                if chip(ui, &crate::onboard::special_name(code), is(Binding::Special(code))).clicked() {
                    pick = Some(BindingChoice::Plain(Binding::Special(code)));
                }
            }
        });
        ui.add_space(10.0);
        group(ui, "buttons.group_macro");
        ui.horizontal_wrapped(|ui| {
            if self.cfg.macros.is_empty() {
                ui.label(egui::RichText::new(tr!("buttons.no_macros")).font(f(12.0)).color(DIM));
            }
            for m in &self.cfg.macros {
                if chip(ui, &m.name, current_macro.as_deref() == Some(m.id.as_str())).clicked() {
                    pick = Some(BindingChoice::Macro(m.id.clone()));
                }
            }
        });
        ui.add_space(10.0);
        group(ui, "buttons.group_shortcut");
        {
            let active = self.shortcut_cap.is_some();
            let text = match (active, current) {
                (true, _) => {
                    let (m, k) = self.shortcut_cap.as_ref().map(|(_, c)| (c.mods, c.key)).unwrap_or((0, 0));
                    if m == 0 && k == 0 { tr!("buttons.press_combo") } else { keys::describe_keystroke(m, k) }
                }
                (false, Binding::Key { key, mods }) if current_macro.is_none() => keys::describe_keystroke(mods, key),
                _ => tr!("buttons.click_to_record"),
            };
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
            let border = if active { ACCENT } else if resp.hovered() { BORDER3 } else { BORDER2 };
            ui.painter().rect(r, 8, INPUT, Stroke::new(1.0, border), egui::StrokeKind::Inside);
            let is_key = matches!(current, Binding::Key { .. }) && current_macro.is_none();
            let color = if active { ACCENT } else if is_key { TEXT } else { DIM };
            paint_text(ui, r.center(), Align2::CENTER_CENTER, &text, if is_key || active { fm(13.0) } else { f(12.0) }, color, r.width() - 16.0);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                if active {
                    self.shortcut_cap = None;
                } else {
                    self.stop_captures();
                    match Capture::start(ui.ctx().clone()) {
                        Ok(c) => self.shortcut_cap = Some((c, ComboBuilder::default())),
                        Err(e) => self.flash_err(e.to_string()),
                    }
                }
            }
            ui.label(egui::RichText::new(tr!("buttons.stored_note")).font(f(11.0)).color(DIM));
        }
        ui.add_space(10.0);
        group(ui, "buttons.group_media");
        ui.horizontal_wrapped(|ui| {
            for (code, name, _) in keys::CONSUMER {
                if chip(ui, &tr!(name), is(Binding::Media(code))).clicked() {
                    pick = Some(BindingChoice::Plain(Binding::Media(code)));
                }
            }
        });
        ui.add_space(10.0);
        group(ui, "buttons.group_other");
        ui.horizontal_wrapped(|ui| {
            let others = [0x0Bu8, 0x01, 0x02, 0x08, 0x09, 0x0A].map(Binding::Special).into_iter().chain([Binding::Disabled]);
            for b in others {
                if chip(ui, &b.describe(), is(b)).clicked() {
                    pick = Some(BindingChoice::Plain(b));
                }
            }
        });
        ui.add_space(8.0);
        if let Some(c) = pick {
            self.shortcut_cap = None;
            self.choose_binding(c);
        }
    }
}
