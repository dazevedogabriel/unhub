//! Página "DPI": níveis, padrão, shift, teste ao vivo e taxa de relatório.

use eframe::egui::{Align2, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::theme::*;
use super::App;
use crate::device::Cmd;
use crate::onboard::DPI_SLOTS;

impl App {
    /// Grava as 5 linhas da interface no perfil: os níveis ativos ficam em sequência.
    fn commit_dpi(&mut self) {
        if !self.dpi_rows.iter().any(|r| r.0) {
            self.dpi_rows[0].0 = true;
        }
        if !self.dpi_rows[self.dpi_def].0 {
            self.dpi_def = self.dpi_rows.iter().position(|r| r.0).unwrap();
        }
        if !self.dpi_rows[self.dpi_shift].0 {
            self.dpi_shift = self.dpi_rows.iter().position(|r| r.0).unwrap();
        }
        let active: Vec<usize> = (0..DPI_SLOTS).filter(|&i| self.dpi_rows[i].0).collect();
        let values: Vec<u32> = active.iter().map(|&i| self.dpi_rows[i].1).collect();
        let pos = |slot: usize| active.iter().position(|&s| s == slot).unwrap_or(0);
        let (d, s) = (pos(self.dpi_def), pos(self.dpi_shift));
        let Some(p) = self.profile.as_mut() else { return };
        let before = p.data.clone();
        let _ = p.set_dpi_levels(&values);
        let _ = p.set_default_dpi_index(d);
        let _ = p.set_shift_dpi_index(s);
        if p.data != before {
            self.dirty = true;
        }
    }

    pub fn dpi_page(&mut self, ui: &mut Ui, area: Rect) {
        let Some(dev) = self.device().cloned() else { return };
        let (lo, hi, step) = dev.dpi_range;
        let live = self.status.map(|s| s.dpi);
        let mut commit = false;

        // --- cartão do DPI atual -----------------------------------------------------------
        let top = Rect::from_min_size(area.min, Vec2::new(area.width(), 150.0));
        paint_card(ui, top);
        let t = Rect::from_min_max(top.min + Vec2::new(24.0, 22.0), top.max - Vec2::new(24.0, 30.0));
        let big = live.map(|v| v.to_string()).unwrap_or("—".into());
        let mut job = caps_job(&big, TEXT);
        job.sections[0].format.font_id = mono(44.0);
        job.sections[0].format.extra_letter_spacing = -1.2;
        let g = ui.painter().layout_job(job);
        let gs = g.size();
        ui.painter().galley(t.left_top(), g, TEXT);
        ui.painter().text(Pos2::new(t.left() + gs.x + 10.0, t.top() + gs.y - 10.0), Align2::LEFT_BOTTOM, tr!("dpi.current"), f(13.0), MUTED);
        let range = tr!("dpi.range", lo = lo, hi = hi, step = step);
        ui.painter().text(Pos2::new(t.right(), t.top() + gs.y - 10.0), Align2::RIGHT_BOTTOM, range, mono(12.0), DIM);
        let rail = Rect::from_min_size(Pos2::new(t.left(), t.bottom() - 26.0), Vec2::new(t.width(), 6.0));
        ui.painter().rect_filled(rail, 3, INPUT);
        let logp = |v: u32| ((v.max(lo) as f32 / lo as f32).ln() / (hi as f32 / lo as f32).ln()).clamp(0.0, 1.0);
        for (i, (on, v)) in self.dpi_rows.iter().enumerate() {
            if !on {
                continue;
            }
            let x = rail.left() + rail.width() * logp(*v);
            let color = if live == Some(*v) { ACCENT } else if i == self.dpi_shift { PURPLE } else { CODE };
            ui.painter().rect_filled(Rect::from_center_size(Pos2::new(x, rail.center().y + 3.0), Vec2::new(4.0, 20.0)), 2, color);
            ui.painter().text(Pos2::new(x, rail.bottom() + 12.0), Align2::CENTER_TOP, v.to_string(), mono(10.0), color);
        }

        // --- tabela de níveis ----------------------------------------------------------------
        let row_h = 56.0;
        let table = Rect::from_min_size(Pos2::new(area.left(), top.bottom() + 20.0), Vec2::new(area.width(), 40.0 + row_h * 5.0));
        paint_card(ui, table);
        let cols_w = [70.0, 60.0, 0.0, 120.0, 90.0, 90.0, 130.0];
        let gap = 16.0;
        let fixed: f32 = cols_w.iter().sum::<f32>() + gap * 6.0 + 40.0;
        let flex = (table.width() - fixed).max(80.0);
        let xs: Vec<f32> = {
            let mut x = table.left() + 20.0;
            cols_w.iter().map(|w| {
                let cur = x;
                x += if *w == 0.0 { flex } else { *w } + gap;
                cur
            }).collect()
        };
        let wcol = |i: usize| if cols_w[i] == 0.0 { flex } else { cols_w[i] };
        let head = Rect::from_min_size(table.min, Vec2::new(table.width(), 40.0));
        ui.painter().line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, BORDER));
        let heads = [tr!("dpi.col_active"), tr!("dpi.col_level"), tr!("dpi.col_range"), "DPI".into(), tr!("dpi.col_default"), tr!("dpi.col_shift")];
        for (i, h) in heads.iter().enumerate() {
            let centered = i >= 4;
            let x = if centered { xs[i] + wcol(i) / 2.0 } else { xs[i] };
            paint_caps(ui, Pos2::new(x, head.center().y), if centered { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER }, h, DIM);
        }
        for i in 0..DPI_SLOTS {
            let r = Rect::from_min_size(Pos2::new(table.left() + 1.0, head.bottom() + row_h * i as f32), Vec2::new(table.width() - 2.0, row_h));
            let (on, mut val) = self.dpi_rows[i];
            if live == Some(val) && on {
                ui.painter().rect_filled(r, 0, alpha(ACCENT, 0.04));
            }
            if i < DPI_SLOTS - 1 {
                ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, ROWLINE));
            }
            let cy = r.center().y;
            let cell = |k: usize, h: f32| Rect::from_min_size(Pos2::new(xs[k], cy - h / 2.0), Vec2::new(wcol(k), h));
            let tr = Rect::from_min_size(Pos2::new(xs[0], cy - 9.0), Vec2::new(30.0, 18.0));
            let tresp = ui.interact(tr, ui.scope_id().with(("dpion", i)), Sense::click());
            paint_toggle(ui, tr, on, tresp.hovered());
            if tresp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked() {
                self.dpi_rows[i].0 = !on;
                commit = true;
            }
            ui.painter().text(Pos2::new(xs[1], cy), Align2::LEFT_CENTER, format!("0{}", i + 1), mono(13.0), if on { TEXT } else { FAINT });
            let mut c = child(ui, cell(2, 20.0));
            if !on {
                c.set_opacity(0.35);
            }
            let mut fv = val as f32;
            let sresp = slider(&mut c, &mut fv, lo as f32..=hi as f32, wcol(2), true);
            if sresp.changed() {
                let snapped = lo + ((fv - lo as f32) / step as f32).round() as u32 * step;
                let snapped = snapped.clamp(lo, hi);
                if snapped != val {
                    self.dpi_rows[i].1 = snapped;
                    val = snapped;
                }
            }
            if sresp.drag_stopped() || (sresp.clicked() && !sresp.dragged()) {
                commit = true;
            }
            let mut c = child(ui, cell(3, 32.0));
            if !on {
                c.set_opacity(0.35);
            }
            if num_edit(&mut c, ("dpival", i), &mut val, lo, hi, step, wcol(3), None) {
                self.dpi_rows[i].1 = val;
                commit = true;
            }
            for (k, (sel, color)) in [(self.dpi_def == i, ACCENT), (self.dpi_shift == i, PURPLE)].into_iter().enumerate() {
                let mut c = child(ui, Rect::from_center_size(Pos2::new(xs[4 + k] + wcol(4 + k) / 2.0, cy), Vec2::splat(18.0)));
                if !on {
                    c.disable();
                    c.set_opacity(0.35);
                }
                if radio_ring(&mut c, sel, color).clicked() {
                    if k == 0 { self.dpi_def = i } else { self.dpi_shift = i }
                    commit = true;
                }
            }
            let in_use = live == Some(val);
            let br = cell(6, 32.0);
            let bresp = ui.interact(br, ui.scope_id().with(("test", i)), Sense::click());
            let border = if in_use || bresp.hovered() { ACCENT } else { BORDER2 };
            ui.painter().rect(br, 7, eframe::egui::Color32::TRANSPARENT, Stroke::new(1.0, border), eframe::egui::StrokeKind::Inside);
            ui.painter().text(br.center(), Align2::CENTER_CENTER, tr!(if in_use { "dpi.in_use" } else { "dpi.test" }), f(12.0),
                if in_use { ACCENT } else { TEXT2 });
            if bresp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked() {
                self.worker.send(Cmd::TestDpi(val));
            }
        }

        // --- taxa de relatório -------------------------------------------------------------------
        let y = table.bottom() + 24.0;
        let lr = paint_caps(ui, Pos2::new(area.left(), y + 18.0), Align2::LEFT_CENTER, &tr!("dpi.rate"), DIM);
        let rates = [125u32, 250, 500, 1000];
        let cur = self.profile.as_ref().map(|p| p.report_rate_hz()).unwrap_or(1000);
        let labels: Vec<String> = rates.iter().map(|r| format!("{r} Hz")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let sel = rates.iter().position(|r| *r == cur).unwrap_or(3);
        let mut c = child(ui, Rect::from_min_size(Pos2::new(lr.right() + 14.0, y), Vec2::new(380.0, 36.0)));
        let seg = segmented(&mut c, &refs, sel, mono(12.0), None, true);
        if let Some(i) = seg {
            if let Some(p) = self.profile.as_mut() {
                if p.report_rate_hz() != rates[i] {
                    let _ = p.set_report_rate_hz(rates[i]);
                    self.dirty = true;
                }
            }
        }
        let hx = lr.right() + 14.0 + 300.0 + 28.0;
        let hint = tr!("dpi.hint");
        let g = ui.painter().layout(hint, f(12.0), DIM, (area.right() - hx).max(200.0));
        ui.painter().galley(Pos2::new(hx, y + 18.0 - g.size().y / 2.0), g, DIM);

        if commit {
            self.commit_dpi();
        }
    }
}
