//! Cores, fontes e widgets próprios que reproduzem o layout do Claude Design (UNHUB.dc.html).

use std::sync::Arc;

use eframe::egui::{
    self, epaint::CornerRadius, text::LayoutJob, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId,
    Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, TextFormat, Ui, Vec2,
};

pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const BG: Color32 = hex(0x111216);
pub const TITLEBAR: Color32 = hex(0x0C0D10);
pub const SIDEBAR: Color32 = hex(0x0E0F12);
pub const CARD: Color32 = hex(0x16181C);
pub const INPUT: Color32 = hex(0x0E0F12);
pub const BORDER: Color32 = hex(0x23262C);
pub const BORDER2: Color32 = hex(0x26292F);
pub const BORDER3: Color32 = hex(0x3A3E46);
pub const LINE: Color32 = hex(0x1C1F24);
pub const ROWLINE: Color32 = hex(0x1E2025);
pub const HOVER: Color32 = hex(0x1A1C21);
pub const CHIP: Color32 = hex(0x1E2025);
pub const SEL: Color32 = hex(0x24272D);
pub const TEXT: Color32 = hex(0xECEDEF);
pub const TEXT2: Color32 = hex(0xC5C8CE);
pub const MUTED: Color32 = hex(0x8B9099);
pub const DIM: Color32 = hex(0x5C616B);
pub const FAINT: Color32 = hex(0x4A4E56);
pub const CODE: Color32 = hex(0x6B707A);
pub const ACCENT: Color32 = hex(0xC6F432);
pub const ACCENT_HI: Color32 = hex(0xD6FF55);
pub const AMBER: Color32 = hex(0xE8B04A);
pub const BLUE: Color32 = hex(0x8BD3FF);
pub const PURPLE: Color32 = hex(0xB69CFF);
pub const RED: Color32 = hex(0xFF7A6B);
pub const REC: Color32 = hex(0xFF5A4A);
pub const PINK: Color32 = hex(0xFF8FB1);
pub const MINT: Color32 = hex(0x7FE3C4);
pub const TRACK_OFF: Color32 = hex(0x2A2D33);

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

// --- fontes --------------------------------------------------------------------------------

pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or("C:\\Windows".into()).join("Fonts");
    let mut load = |key: &str, files: &[&str]| -> bool {
        for f in files {
            if let Ok(b) = std::fs::read(dir.join(f)) {
                defs.font_data.insert(key.into(), Arc::new(FontData::from_owned(b)));
                return true;
            }
        }
        false
    };
    let ui = load("ui", &["segoeui.ttf"]);
    let medium = load("medium", &["seguisb.ttf", "segoeui.ttf"]);
    let bold = load("bold", &["segoeuib.ttf", "seguisb.ttf"]);
    let mono = load("mono", &["CascadiaMono.ttf", "consola.ttf"]);
    let monob = load("monob", &["consolab.ttf", "CascadiaMono.ttf"]);
    let sym = load("sym", &["seguisym.ttf"]);

    let base_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let base_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let family = |first: &str, ok: bool, base: &Vec<String>| -> Vec<String> {
        let mut v = Vec::new();
        if ok {
            v.push(first.to_string());
        }
        if ui {
            v.push("ui".into());
        }
        v.extend(base.iter().cloned());
        if sym {
            v.push("sym".into());
        }
        v
    };
    defs.families.insert(FontFamily::Proportional, family("ui", ui, &base_prop));
    defs.families.insert(FontFamily::Name("medium".into()), family("medium", medium, &base_prop));
    defs.families.insert(FontFamily::Name("bold".into()), family("bold", bold, &base_prop));
    defs.families.insert(FontFamily::Monospace, family("mono", mono, &base_mono));
    defs.families.insert(FontFamily::Name("monob".into()), family("monob", monob, &base_mono));
    ctx.set_fonts(defs);
}

pub fn f(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn fm(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
pub fn fb(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
pub fn monob(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("monob".into()))
}

pub fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        v.extreme_bg_color = INPUT;
        v.text_edit_bg_color = Some(INPUT);
        v.faint_bg_color = CHIP;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = alpha(ACCENT, 0.28);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.text_cursor.stroke = Stroke::new(1.5, ACCENT);
        for (w, fill, stroke) in [
            (&mut v.widgets.noninteractive, CARD, BORDER),
            (&mut v.widgets.inactive, INPUT, BORDER2),
            (&mut v.widgets.hovered, HOVER, BORDER3),
            (&mut v.widgets.active, SEL, ACCENT),
            (&mut v.widgets.open, INPUT, BORDER3),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, stroke);
            w.corner_radius = CornerRadius::same(8);
            w.fg_stroke = Stroke::new(1.0, TEXT2);
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.popup_shadow = egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: alpha(Color32::BLACK, 0.5) };
        v.window_shadow = egui::Shadow { offset: [0, 20], blur: 60, spread: 0, color: alpha(Color32::BLACK, 0.6) };
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(10.0, 6.0);
        s.spacing.interact_size = Vec2::new(24.0, 28.0);
        s.spacing.scroll.floating = true;
        s.spacing.scroll.bar_width = 6.0;
        s.text_styles.insert(egui::TextStyle::Body, f(14.0));
        s.text_styles.insert(egui::TextStyle::Button, f(13.0));
        s.text_styles.insert(egui::TextStyle::Small, f(11.0));
        s.text_styles.insert(egui::TextStyle::Monospace, mono(13.0));
    });
}

// --- texto -----------------------------------------------------------------------------------

/// Uma linha que corta com "…" se passar de `max_w`.
pub fn job(text: &str, font: FontId, color: Color32, max_w: f32) -> LayoutJob {
    let mut j = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, color));
    j.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    j
}

pub fn paint_text(ui: &Ui, pos: Pos2, anchor: Align2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let g = ui.painter().layout_job(job(text, font, color, max_w));
    let rect = anchor.anchor_size(pos, g.size());
    ui.painter().galley(rect.min, g, color);
    rect
}

/// Rótulo em caixa alta, mono 10, espaçado (cabeçalhos do layout).
pub fn caps_job(text: &str, color: Color32) -> LayoutJob {
    let mut fmt = TextFormat::simple(mono(10.0), color);
    fmt.extra_letter_spacing = 0.7;
    LayoutJob::single_section(text.to_string(), fmt)
}

pub fn paint_caps(ui: &Ui, pos: Pos2, anchor: Align2, text: &str, color: Color32) -> Rect {
    let g = ui.painter().layout_job(caps_job(text, color));
    let rect = anchor.anchor_size(pos, g.size());
    ui.painter().galley(rect.min, g, color);
    rect
}

pub fn caps(ui: &mut Ui, text: &str) -> Response {
    ui.label(caps_job(text, DIM))
}

pub fn text_size(ui: &Ui, text: &str, font: FontId) -> Vec2 {
    ui.painter().layout_no_wrap(text.to_string(), font, TEXT).size()
}

// --- containers ------------------------------------------------------------------------------

pub fn paint_card(ui: &Ui, rect: Rect) {
    ui.painter().rect(rect, 14, CARD, Stroke::new(1.0, BORDER), StrokeKind::Inside);
}

pub fn child(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)))
}

pub fn input_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(INPUT)
        .stroke(Stroke::new(1.0, BORDER2))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 8))
}

// --- widgets ---------------------------------------------------------------------------------

pub fn paint_toggle(ui: &Ui, rect: Rect, on: bool, hovered: bool) {
    let t = ui.ctx().animate_bool(ui.scope_id().with(rect.min.x as i32).with(rect.min.y as i32), on);
    let track = lerp_color(TRACK_OFF, ACCENT, t);
    let track = if hovered && !on { hex(0x33363D) } else { track };
    let r = rect.height() / 2.0;
    ui.painter().rect_filled(rect, r, track);
    let knob = rect.height() - 4.0;
    let x = egui::lerp(rect.left() + 2.0..=rect.right() - 2.0 - knob, t);
    ui.painter().circle_filled(Pos2::new(x + knob / 2.0, rect.center().y), knob / 2.0, SIDEBAR);
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Outline,
    Primary,
    Dashed,
    Danger,
}

pub fn button(ui: &mut Ui, label: &str, kind: Btn) -> Response {
    button_sized(ui, label, kind, None, 13.0)
}

pub fn button_sized(ui: &mut Ui, label: &str, kind: Btn, width: Option<f32>, size: f32) -> Response {
    let font = if kind == Btn::Primary { fb(size) } else { f(size) };
    let ts = text_size(ui, label, font.clone());
    let w = width.unwrap_or(ts.x + if kind == Btn::Primary { 32.0 } else { 28.0 });
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ts.y + 16.0), Sense::click());
    let enabled = ui.is_enabled();
    let hov = resp.hovered() && enabled;
    let p = ui.painter();
    let (fill, stroke, color) = match kind {
        Btn::Primary => (if hov { ACCENT_HI } else { ACCENT }, Stroke::NONE, SIDEBAR),
        Btn::Outline => (if hov { HOVER } else { Color32::TRANSPARENT }, Stroke::new(1.0, BORDER2), TEXT2),
        Btn::Danger => (if hov { hex(0x2A1A1A) } else { Color32::TRANSPARENT }, Stroke::new(1.0, BORDER2), RED),
        Btn::Dashed => (Color32::TRANSPARENT, Stroke::NONE, if hov { TEXT } else { MUTED }),
    };
    p.rect(rect, 8, fill, stroke, StrokeKind::Inside);
    if kind == Btn::Dashed {
        dashed_rect(ui, rect.shrink(0.5), hex(0x33373E));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

pub fn dashed_rect(ui: &Ui, r: Rect, color: Color32) {
    let s = Stroke::new(1.0, color);
    let p = ui.painter();
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        p.extend(egui::Shape::dashed_line(&[w[0], w[1]], s, 4.0, 3.0));
    }
}

pub fn chip(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let font = f(12.0);
    let ts = text_size(ui, label, font.clone());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ts.x + 20.0, ts.y + 14.0), Sense::click());
    let hov = resp.hovered();
    let (fill, border, color) = if selected {
        (ACCENT, ACCENT, SIDEBAR)
    } else {
        (CHIP, if hov { BORDER3 } else { BORDER2 }, TEXT2)
    };
    ui.painter().rect(rect, 7, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Botão "+ Ação" da barra de adicionar.
pub fn add_chip(ui: &mut Ui, label: &str) -> Response {
    let font = f(12.0);
    let text = format!("+ {label}");
    let ts = text_size(ui, &text, font.clone());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ts.x + 24.0, ts.y + 14.0), Sense::click());
    let hov = resp.hovered();
    ui.painter().rect(rect, 7, CHIP, Stroke::new(1.0, if hov { ACCENT } else { BORDER2 }), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, if hov { TEXT } else { TEXT2 });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Controle segmentado (Normal / G-Shift, taxa de relatório...). Retorna o índice clicado.
pub fn segmented(ui: &mut Ui, items: &[&str], selected: usize, font: FontId, fill_width: Option<f32>, accent_text: bool) -> Option<usize> {
    let pad_x = 12.0;
    let sizes: Vec<Vec2> = items.iter().map(|s| text_size(ui, s, font.clone())).collect();
    let h = sizes.iter().map(|s| s.y).fold(0.0, f32::max) + 14.0 + 6.0;
    let natural: f32 = sizes.iter().map(|s| s.x + 2.0 * pad_x).sum::<f32>() + 6.0 + 2.0 * (items.len() as f32 - 1.0);
    let w = fill_width.unwrap_or(natural);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let bg = if accent_text { CARD } else { INPUT };
    ui.painter().rect(rect, 9, bg, if accent_text { Stroke::new(1.0, BORDER) } else { Stroke::NONE }, StrokeKind::Inside);
    let inner = rect.shrink(3.0);
    let mut clicked = None;
    let mut x = inner.left();
    for (i, (label, s)) in items.iter().zip(&sizes).enumerate() {
        let iw = if fill_width.is_some() {
            (inner.width() - 2.0 * (items.len() as f32 - 1.0)) / items.len() as f32
        } else {
            s.x + 2.0 * pad_x
        };
        let r = Rect::from_min_size(Pos2::new(x, inner.top()), Vec2::new(iw, inner.height()));
        let resp = ui.interact(r, ui.scope_id().with(("seg", label, rect.min.x as i32, rect.min.y as i32)), Sense::click());
        let sel = i == selected;
        if sel {
            ui.painter().rect_filled(r, 7, SEL);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 7, alpha(SEL, 0.5));
        }
        let color = if sel { if accent_text { ACCENT } else { TEXT } } else { MUTED };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, font.clone(), color);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
        x += iw + 2.0;
    }
    clicked
}

pub fn radio_ring(ui: &mut Ui, selected: bool, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    let ring = if selected { color } else if resp.hovered() { hex(0x55595F) } else { BORDER3 };
    ui.painter().circle_stroke(rect.center(), 8.25, Stroke::new(1.5, ring));
    if selected {
        ui.painter().circle_filled(rect.center(), 4.0, color);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn checkbox(ui: &mut Ui, on: bool, label: &str) -> Response {
    let font = f(12.0);
    let ts = text_size(ui, label, font.clone());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(14.0 + 8.0 + ts.x, ts.y.max(14.0)), Sense::click());
    let b = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 7.0), Vec2::splat(14.0));
    ui.painter().rect(b, 4, if on { ACCENT } else { Color32::TRANSPARENT }, Stroke::new(1.0, BORDER3), StrokeKind::Inside);
    if on {
        ui.painter().text(b.center(), Align2::CENTER_CENTER, "✓", fb(10.0), SIDEBAR);
    }
    ui.painter().text(Pos2::new(b.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, label, font,
        if resp.hovered() { TEXT } else { MUTED });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Slider no estilo do input range (trilho cinza, parte preenchida e botão verde-limão).
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32, log: bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    let to_t = |v: f32| if log { (v / lo).ln() / (hi / lo).ln() } else { (v - lo) / (hi - lo) };
    let from_t = |t: f32| if log { lo * (hi / lo).powf(t) } else { lo + (hi - lo) * t };
    let rail = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 14.0, 4.0));
    if ui.is_enabled() {
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.clicked() || resp.drag_started() {
                let t = ((p.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
                let nv = from_t(t);
                if (nv - *value).abs() > f32::EPSILON {
                    *value = nv;
                    resp.mark_changed();
                }
            }
        }
    }
    let t = to_t(value.clamp(lo, hi)).clamp(0.0, 1.0);
    let x = rail.left() + rail.width() * t;
    let p = ui.painter();
    p.rect_filled(rail, 2, hex(0x3A3D44));
    p.rect_filled(Rect::from_min_max(rail.left_top(), Pos2::new(x, rail.bottom())), 2, ACCENT);
    let hov = resp.hovered() || resp.dragged();
    p.circle_filled(Pos2::new(x, rect.center().y), if hov { 8.0 } else { 7.0 }, ACCENT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Campo numérico: edita como texto e só confirma ao sair do campo ou com Enter.
pub fn num_edit(ui: &mut Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, value: &mut u32, min: u32, max: u32, step: u32, width: f32, suffix: Option<&str>) -> bool {
    let id = ui.scope_id().with(id_salt);
    let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.to_string());
    let mut changed = false;
    let frame = input_frame().inner_margin(Margin { left: 10, right: if suffix.is_some() { 34 } else { 10 }, top: 7, bottom: 7 });
    let resp = ui.add_sized(
        Vec2::new(width, 32.0),
        egui::TextEdit::singleline(&mut buf).font(mono(13.0)).frame(frame).text_color(TEXT),
    );
    if let Some(sfx) = suffix {
        ui.painter().text(Pos2::new(resp.rect.right() - 10.0, resp.rect.center().y), Align2::RIGHT_CENTER, sfx, mono(12.0), DIM);
    }
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if resp.lost_focus() {
        if let Ok(v) = buf.trim().parse::<u64>() {
            let mut v = (v.min(max as u64) as u32).max(min);
            if step > 1 {
                v = min + ((v - min) as f32 / step as f32).round() as u32 * step;
                v = v.min(max);
            }
            if v != *value {
                *value = v;
                changed = true;
            }
        }
    }
    changed
}

pub fn dot(ui: &Ui, center: Pos2, r: f32, color: Color32, glow: bool) {
    if glow {
        for i in 1..=4 {
            ui.painter().circle_filled(center, r + i as f32 * 2.0, alpha(color, 0.10 / i as f32));
        }
    }
    ui.painter().circle_filled(center, r, color);
}

/// Brilho difuso (box-shadow do layout) ao redor de um retângulo arredondado.
pub fn glow(ui: &Ui, rect: Rect, radius: f32, color: Color32, strength: f32) {
    let layers = 14;
    for i in (1..=layers).rev() {
        let e = i as f32 * 3.2;
        let a = strength * 0.10 * (1.0 - i as f32 / (layers as f32 + 1.0)).powf(1.6);
        ui.painter().rect_filled(rect.expand(e), (radius + e) as u8, alpha(color, a));
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let c = egui::ecolor::Hsva::new(h.rem_euclid(1.0), s, v, 1.0);
    let [r, g, b] = c.to_srgb();
    Color32::from_rgb(r, g, b)
}
