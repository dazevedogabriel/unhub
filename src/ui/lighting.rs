//! Página "Iluminação": efeito por zona, gravado no perfil onboard.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::theme::*;
use super::App;
use crate::onboard::{LedEffect, FX_BREATH, FX_CYCLE, FX_FIXED, FX_OFF, LED_ZONES};
use crate::util;

const EFFECTS: [(u8, &str); 4] = [(FX_OFF, "Desligado"), (FX_FIXED, "Fixo"), (FX_BREATH, "Respiração"), (FX_CYCLE, "Ciclo de cores")];
const SWATCHES: [u32; 6] = [0x00FF00, 0xC6F432, 0x00D1FF, 0x7A5CFF, 0xFF2D6F, 0xFFFFFF];

fn effect_name(e: &LedEffect) -> String {
    EFFECTS.iter().find(|x| x.0 == e.effect).map(|x| x.1.to_string()).unwrap_or(format!("Outro (0x{:02x})", e.effect))
}

impl App {
    fn set_zone(&mut self, zone: usize, e: LedEffect) {
        let sync = self.sync_zones;
        let Some(p) = self.profile.as_mut() else { return };
        let zones: Vec<usize> = if sync { (0..LED_ZONES).collect() } else { vec![zone] };
        for z in zones {
            if p.led(z) != e && p.set_led(z, &e).is_ok() {
                self.dirty = true;
            }
        }
    }

    pub fn light_page(&mut self, ui: &mut Ui, area: Rect) {
        let Some(dev) = self.device().cloned() else { return };
        let Some(p) = self.profile.clone() else { return };
        let zones = util::zones(dev.pid, LED_ZONES);

        // --- sincronizar zonas ----------------------------------------------------------------
        let sr = Rect::from_min_size(area.min, Vec2::new(area.width(), 64.0));
        let sresp = ui.interact(sr, ui.scope_id().with("sync"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().rect(sr, 12, CARD, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        ui.painter().text(Pos2::new(sr.left() + 18.0, sr.center().y - 2.0), Align2::LEFT_BOTTOM,
            "Mesma configuração em todas as zonas", fm(14.0), TEXT);
        ui.painter().text(Pos2::new(sr.left() + 18.0, sr.center().y + 2.0), Align2::LEFT_TOP,
            format!("{} segue a {}", zones[1].1, zones[0].1), f(12.0), MUTED);
        paint_toggle(ui, Rect::from_center_size(Pos2::new(sr.right() - 36.0, sr.center().y), Vec2::new(36.0, 20.0)), self.sync_zones, sresp.hovered());
        if sresp.clicked() {
            self.sync_zones = !self.sync_zones;
            if self.sync_zones {
                self.set_zone(zones[0].0, p.led(zones[0].0));
            }
        }

        // --- cartões das zonas -------------------------------------------------------------------
        let top = sr.bottom() + 20.0;
        let cw = (area.width() - 20.0) / 2.0;
        let mut animating = false;
        for (d, (z, name)) in zones.iter().enumerate() {
            let z = *z;
            let card = Rect::from_min_size(Pos2::new(area.left() + d as f32 * (cw + 20.0), top), Vec2::new(cw, 380.0));
            let locked = self.sync_zones && d == 1;
            let e = if locked { p.led(zones[0].0) } else { p.led(z) };
            let mut c = child(ui, card);
            if locked {
                c.set_opacity(0.45);
                c.disable();
            }
            paint_card(&c, card);
            animating |= self.zone_card(&mut c, card, z, name, e);
        }
        ui.painter().text(Pos2::new(area.left(), top + 380.0 + 20.0), Align2::LEFT_TOP,
            "Gravado na memória do mouse junto com o perfil. Zonas na ordem informada pelo mouse (feature 0x8070).",
            f(12.0), DIM);
        if animating {
            ui.ctx().request_repaint();
        }
    }

    /// Retorna true se a prévia está animando.
    fn zone_card(&mut self, ui: &mut Ui, card: Rect, z: usize, name: &str, e: LedEffect) -> bool {
        let mut new = e;
        // prévia
        let pv = Rect::from_min_size(card.min, Vec2::new(card.width(), 150.0));
        {
            let p = ui.painter();
            p.rect_filled(pv.shrink(1.0), egui::CornerRadius { nw: 13, ne: 13, sw: 0, se: 0 }, hex(0x0B0C0E));
            p.line_segment([pv.left_bottom(), pv.right_bottom()], Stroke::new(1.0, BORDER));
        }
        paint_caps(ui, Pos2::new(pv.left() + 16.0, pv.top() + 14.0), Align2::LEFT_TOP, "ZONA", DIM);
        ui.painter().text(Pos2::new(pv.left() + 16.0, pv.top() + 28.0), Align2::LEFT_TOP, name, fm(18.0), TEXT);
        ui.painter().text(Pos2::new(pv.right() - 16.0, pv.top() + 14.0), Align2::RIGHT_TOP, effect_name(&e), mono(11.0), MUTED);
        let t = self.started.elapsed().as_secs_f32();
        let period = e.period_ms as f32 / 1000.0;
        let off = e.effect == FX_OFF;
        let base = Color32::from_rgb(e.color[0], e.color[1], e.color[2]);
        let (color, opacity, anim) = match e.effect {
            FX_FIXED => (base, 1.0, false),
            FX_BREATH => {
                let k = 0.5 - 0.5 * (t / period * std::f32::consts::TAU).cos();
                (base, (0.25 + 0.75 * k) * e.brightness as f32 / 100.0, true)
            }
            FX_CYCLE => (hsv(t / period, 1.0, 1.0), e.brightness as f32 / 100.0, true),
            _ => (CHIP, 1.0, false),
        };
        let logo = name == "Logo";
        let shape = if logo { Vec2::new(54.0, 54.0) } else { Vec2::new(120.0, 10.0) };
        let radius = if logo { 16.0 } else { 5.0 };
        let sr = Rect::from_center_size(pv.center() + Vec2::new(0.0, 10.0), shape);
        if off || e.raw.is_some() {
            ui.painter().rect_filled(sr, radius as u8, CHIP);
        } else {
            let o = opacity.max(0.08);
            glow(ui, sr, radius, color, o);
            ui.painter().rect_filled(sr, radius as u8, alpha(color, o));
        }

        // controles
        let body = Rect::from_min_max(Pos2::new(card.left() + 18.0, pv.bottom() + 18.0), card.max - Vec2::new(18.0, 18.0));
        let label_w = 110.0;
        let row = |i: usize| Rect::from_min_size(Pos2::new(body.left(), body.top() + i as f32 * 52.0), Vec2::new(body.width(), 36.0));
        let ctrl_x = body.left() + label_w;
        let ctrl_w = body.width() - label_w;
        let uses_color = matches!(e.effect, FX_FIXED | FX_BREATH);
        let uses_period = matches!(e.effect, FX_CYCLE | FX_BREATH);

        // efeito
        let r0 = row(0);
        ui.painter().text(Pos2::new(r0.left(), r0.center().y), Align2::LEFT_CENTER, "Efeito", f(13.0), MUTED);
        {
            let mut c = child(ui, Rect::from_min_size(Pos2::new(ctrl_x, r0.top()), Vec2::new(ctrl_w, 36.0)));
            c.spacing_mut().button_padding = Vec2::new(10.0, 8.0);
            let mut cur = e.effect;
            egui::ComboBox::from_id_salt(("effect", z))
                .width(ctrl_w - 22.0)
                .selected_text(egui::RichText::new(effect_name(&e)).font(fm(13.0)).color(TEXT))
                .show_ui(&mut c, |ui| {
                    for (id, label) in EFFECTS {
                        ui.selectable_value(&mut cur, id, egui::RichText::new(label).font(f(13.0)));
                    }
                });
            if cur != e.effect {
                new = LedEffect { effect: cur, raw: None, ..e };
                if cur == FX_BREATH && !(1000..=20000).contains(&new.period_ms) {
                    new.period_ms = 4000;
                }
                if cur == FX_CYCLE && !(1000..=20000).contains(&new.period_ms) {
                    new.period_ms = 8000;
                }
                new.brightness = new.brightness.clamp(1, 100);
            }
        }

        // cor
        let r1 = row(1);
        {
            let mut c = child(ui, r1);
            if !uses_color {
                c.set_opacity(0.35);
                c.disable();
            }
            c.painter().text(Pos2::new(r1.left(), r1.center().y), Align2::LEFT_CENTER, "Cor", f(13.0), MUTED);
            let mut x = ctrl_x;
            for sw in SWATCHES {
                let col = hex(sw);
                let sr = Rect::from_center_size(Pos2::new(x + 11.0, r1.center().y), Vec2::splat(22.0));
                let resp = c.interact(sr, c.scope_id().with(("sw", z, sw)), Sense::click());
                let on = e.color == [col.r(), col.g(), col.b()];
                if on {
                    c.painter().rect_stroke(sr.expand(3.5), 9, Stroke::new(1.5, TEXT), egui::StrokeKind::Inside);
                }
                c.painter().rect_filled(sr, 6, col);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    new.color = [col.r(), col.g(), col.b()];
                }
                x += 30.0;
            }
            let pr = Rect::from_center_size(Pos2::new(x + 11.0, r1.center().y), Vec2::splat(22.0));
            let presp = c.interact(pr, c.scope_id().with(("pick", z)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            dashed_rect(&c, pr, if presp.hovered() { TEXT } else { BORDER3 });
            c.painter().text(pr.center(), Align2::CENTER_CENTER, "+", f(14.0), MUTED);
            let mut col = Color32::from_rgb(e.color[0], e.color[1], e.color[2]);
            egui::Popup::from_toggle_button_response(&presp)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    if egui::color_picker::color_picker_color32(ui, &mut col, egui::color_picker::Alpha::Opaque) {
                        new.color = [col.r(), col.g(), col.b()];
                    }
                });
            c.painter().text(Pos2::new(r1.right(), r1.center().y), Align2::RIGHT_CENTER,
                format!("#{:02X}{:02X}{:02X}", e.color[0], e.color[1], e.color[2]), mono(12.0), MUTED);
        }

        // ciclo e brilho
        for (k, label) in [(2usize, "Ciclo"), (3, "Brilho")] {
            let r = row(k);
            let mut c = child(ui, r);
            if !uses_period {
                c.set_opacity(0.35);
                c.disable();
            }
            c.painter().text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, label, f(13.0), MUTED);
            let sw = ctrl_w - 60.0;
            let mut sc = child(&mut c, Rect::from_min_size(Pos2::new(ctrl_x, r.center().y - 9.0), Vec2::new(sw, 18.0)));
            if k == 2 {
                let mut v = (e.period_ms as f32 / 1000.0).clamp(1.0, 20.0);
                if slider(&mut sc, &mut v, 1.0..=20.0, sw, false).changed() {
                    new.period_ms = (v.round() as u16).clamp(1, 20) * 1000;
                }
                c.painter().text(Pos2::new(r.right(), r.center().y), Align2::RIGHT_CENTER,
                    format!("{} s", e.period_ms / 1000), mono(12.0), TEXT);
            } else {
                let mut v = e.brightness as f32;
                if slider(&mut sc, &mut v, 1.0..=100.0, sw, false).changed() {
                    new.brightness = v.round().clamp(1.0, 100.0) as u8;
                }
                c.painter().text(Pos2::new(r.right(), r.center().y), Align2::RIGHT_CENTER,
                    format!("{}%", e.brightness), mono(12.0), TEXT);
            }
        }

        if new != e {
            self.set_zone(z, new);
        }
        anim
    }
}
