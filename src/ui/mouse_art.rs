//! Desenho do mouse (vista de cima) com áreas clicáveis por botão, como no layout.

use std::f32::consts::PI;

use eframe::egui::{self, epaint::Vertex, Align2, Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use super::theme::*;

const W: f32 = 200.0;
const H: f32 = 320.0;

/// Contorno do corpo: metade de cima elíptica (100×130), base mais longa (96×190).
fn body_outline() -> Vec<Pos2> {
    let mut pts = Vec::new();
    let n = 40;
    for i in 0..=n {
        let t = PI + PI * i as f32 / n as f32;
        pts.push(Pos2::new(100.0 + 100.0 * t.cos(), 130.0 + 130.0 * t.sin()));
    }
    for i in 1..=n / 2 {
        let t = (PI / 2.0) * i as f32 / (n / 2) as f32;
        pts.push(Pos2::new(104.0 + 96.0 * t.cos(), 130.0 + 190.0 * t.sin()));
    }
    for i in 0..=n / 2 {
        let t = PI / 2.0 + (PI / 2.0) * i as f32 / (n / 2) as f32;
        pts.push(Pos2::new(96.0 + 96.0 * t.cos(), 130.0 + 190.0 * t.sin()));
    }
    pts
}

/// Botão esquerdo (lado = -1) ou direito (+1): quarto da elipse de cima, até o vão central.
fn top_button(side: f32) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let n = 24;
    let (a0, a1) = if side < 0.0 { (PI, 1.5 * PI) } else { (1.5 * PI, 2.0 * PI) };
    for i in 0..=n {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        let mut p = Pos2::new(100.0 + 100.0 * t.cos(), 130.0 + 130.0 * t.sin());
        if side < 0.0 { p.x = p.x.min(99.0) } else { p.x = p.x.max(101.0) }
        pts.push(p);
    }
    pts.push(Pos2::new(if side < 0.0 { 99.0 } else { 101.0 }, 130.0));
    if side > 0.0 {
        // fecha voltando pelo vão central
        pts.rotate_right(1);
    }
    pts
}

struct Hot {
    kind: HotKind,
    tag_dx: f32,
}

enum HotKind {
    Poly(Vec<Pos2>),
    Round(Rect, f32),
}

fn hotspots() -> Vec<Hot> {
    let r = |x: f32, y: f32, w: f32, h: f32, rad: f32| HotKind::Round(Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)), rad);
    vec![
        Hot { kind: HotKind::Poly(top_button(-1.0)), tag_dx: 30.0 },
        Hot { kind: HotKind::Poly(top_button(1.0)), tag_dx: 151.0 },
        Hot { kind: r(88.0, 34.0, 24.0, 56.0, 12.0), tag_dx: 118.0 },
        Hot { kind: r(-9.0, 172.0, 14.0, 44.0, 6.0), tag_dx: -39.0 },
        Hot { kind: r(-9.0, 122.0, 14.0, 44.0, 6.0), tag_dx: -39.0 },
        Hot { kind: r(91.0, 102.0, 18.0, 22.0, 5.0), tag_dx: 115.0 },
    ]
}

fn contains_poly(pts: &[Pos2], p: Pos2) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Desenha o G403 em `rect` (centralizado). Retorna o índice do botão clicado.
pub fn draw(ui: &mut Ui, rect: Rect, selected: usize, count: usize) -> Option<usize> {
    let scale = (rect.height() / (H + 30.0)).min(rect.width() / (W + 50.0)).min(1.2);
    let origin = rect.center() - Vec2::new(W, H) * scale / 2.0;
    let tf = |p: Pos2| origin + p.to_vec2() * scale;
    let painter = ui.painter().clone();

    // corpo com degradê vertical
    let outline: Vec<Pos2> = body_outline().into_iter().map(tf).collect();
    let mut mesh = Mesh::default();
    let center = tf(Pos2::new(100.0, 170.0));
    let col = |p: Pos2| lerp_color(hex(0x22252B), hex(0x191B20), ((p.y - origin.y) / (H * scale)).clamp(0.0, 1.0));
    mesh.vertices.push(Vertex { pos: center, uv: egui::epaint::WHITE_UV, color: col(center) });
    for p in &outline {
        mesh.vertices.push(Vertex { pos: *p, uv: egui::epaint::WHITE_UV, color: col(*p) });
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.indices.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % n]);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(outline, Stroke::new(1.0, hex(0x2C3037))));

    let hots = hotspots();
    let resp = ui.interact(rect, ui.scope_id().with("mouse-art"), Sense::click());
    let mut hovered = None;
    if let Some(pp) = resp.hover_pos() {
        let local = Pos2::new((pp.x - origin.x) / scale, (pp.y - origin.y) / scale);
        // z-order: roda e botão de DPI por cima
        for i in [2usize, 5, 0, 1, 3, 4] {
            if i >= count {
                continue;
            }
            let inside = match &hots[i].kind {
                HotKind::Poly(pts) => contains_poly(pts, local),
                HotKind::Round(r, _) => r.expand(2.0).contains(local),
            };
            if inside {
                hovered = Some(i);
                break;
            }
        }
    }
    if hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    for i in [0usize, 1, 3, 4, 2, 5] {
        if i >= count {
            continue;
        }
        let small = i == 2 || i == 5;
        let sel = i == selected;
        let fill = match (sel, small) {
            (true, true) => hex(0x3A4A12),
            (true, false) => alpha(ACCENT, 0.16),
            (false, true) => TRACK_OFF,
            (false, false) => alpha(Color32::WHITE, 0.02),
        };
        let border = if sel || hovered == Some(i) { ACCENT } else { hex(0x30343B) };
        let tag_y;
        match &hots[i].kind {
            HotKind::Poly(pts) => {
                let pts: Vec<Pos2> = pts.iter().map(|p| tf(*p)).collect();
                tag_y = tf(Pos2::new(0.0, 65.0)).y;
                painter.add(Shape::convex_polygon(pts, fill, Stroke::new(1.0, border)));
            }
            HotKind::Round(r, rad) => {
                let rr = Rect::from_min_max(tf(r.min), tf(r.max));
                tag_y = rr.center().y;
                painter.rect(rr, (rad * scale) as u8, fill, Stroke::new(1.0, border), egui::StrokeKind::Inside);
            }
        }
        let tx = origin.x + hots[i].tag_dx * scale;
        painter.text(Pos2::new(tx, tag_y), Align2::LEFT_CENTER, format!("G{}", i + 1), monob(10.0),
            if sel { ACCENT } else { CODE });
    }

    if resp.clicked() { hovered } else { None }
}
