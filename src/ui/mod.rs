//! Interface gráfica (egui), seguindo o layout UNHUB do Claude Design.

mod buttons;
mod dialogs;
mod dpi;
mod lighting;
mod macros_page;
mod mouse_art;
pub mod theme;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, ViewportCommand};

use crate::agent;
use crate::capture::{Capture, ComboBuilder};
use crate::config::Config;
use crate::device::{Cmd, DeviceInfo, Event, Worker};
use crate::macros::{ListName, Step};
use crate::mouse::Status;
use crate::onboard::{Binding, Profile, DPI_SLOTS};
use crate::util;
use dialogs::Dialog;
use theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Buttons,
    Macros,
    Dpi,
    Light,
}

const TABS: [(Tab, &str, &str); 4] = [
    (Tab::Buttons, "Atalhos", "Atribua uma ação a cada botão"),
    (Tab::Macros, "Macros", "Sequências executadas pelo agente"),
    (Tab::Dpi, "DPI", "Níveis de sensibilidade do sensor"),
    (Tab::Light, "Iluminação", "Efeitos de LED por zona"),
];

enum Conn {
    Searching,
    Missing(String),
    Ready(DeviceInfo),
}

pub struct Recording {
    cap: Capture,
    buf: Vec<Step>,
    down: HashSet<u16>,
    last_ms: Option<u32>,
}

pub struct App {
    worker: Worker,
    conn: Conn,
    profile: Option<Profile>,
    status: Option<Status>,
    dirty: bool,
    busy: Option<&'static str>,

    cfg: Config,
    cfg_dirty: bool,
    cfg_changed_at: Instant,

    tab: Tab,
    gshift: bool,
    sel_button: usize,
    sel_macro: usize,
    sel_list: ListName,
    sel_step: Option<usize>,
    rec: Option<Recording>,
    real_delays: bool,
    shortcut_cap: Option<(Capture, ComboBuilder)>,

    dpi_rows: [(bool, u32); DPI_SLOTS],
    dpi_def: usize,
    dpi_shift: usize,
    sync_zones: bool,

    toast: Option<(String, Instant, bool)>,
    dialog: Option<Dialog>,
    allow_close: bool,

    agent_running: bool,
    legacy_agent: bool,
    autostart: bool,
    ghub: bool,
    last_poll: Instant,
    started: Instant,
    logo: egui::TextureHandle,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        install_fonts(&cc.egui_ctx);
        install_style(&cc.egui_ctx);
        let cfg = Config::load();
        Self {
            worker: Worker::spawn(cc.egui_ctx.clone()),
            conn: Conn::Searching,
            profile: None,
            status: None,
            dirty: false,
            busy: None,
            cfg,
            cfg_dirty: false,
            cfg_changed_at: Instant::now(),
            tab: Tab::Buttons,
            gshift: false,
            sel_button: 0,
            sel_macro: 0,
            sel_list: ListName::Held,
            sel_step: None,
            rec: None,
            real_delays: true,
            shortcut_cap: None,
            dpi_rows: [(false, 800); DPI_SLOTS],
            dpi_def: 0,
            dpi_shift: 0,
            sync_zones: false,
            toast: None,
            dialog: None,
            allow_close: false,
            agent_running: agent::is_running(),
            legacy_agent: agent::legacy_running(),
            autostart: agent::autostart_enabled(),
            ghub: util::ghub_running(),
            last_poll: Instant::now(),
            started: Instant::now(),
            logo: cc.egui_ctx.load_texture(
                "logo",
                egui::ColorImage::from_rgba_unmultiplied([128, 128], crate::icon::LOGO_128),
                egui::TextureOptions::LINEAR,
            ),
        }
    }

    fn device(&self) -> Option<&DeviceInfo> {
        match &self.conn {
            Conn::Ready(d) => Some(d),
            _ => None,
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), false));
    }

    pub fn flash_err(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), true));
    }

    fn set_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn cfg_changed(&mut self) {
        self.cfg_dirty = true;
        self.cfg_changed_at = Instant::now();
    }

    fn save_cfg_now(&mut self) {
        if let Err(e) = self.cfg.save() {
            self.flash_err(format!("Não foi possível salvar a configuração: {e}"));
        }
        self.cfg_dirty = false;
    }

    /// Para qualquer captura de teclado em andamento (só pode haver um hook por processo).
    fn stop_captures(&mut self) {
        self.shortcut_cap = None;
        self.stop_record();
        if let Some(Dialog::Step(d)) = &mut self.dialog {
            d.cap = None;
        }
    }

    // --- eventos do mouse ------------------------------------------------------------------

    fn load_profile(&mut self, p: Profile) {
        if let Some(d) = self.device() {
            self.cfg.prune(d.pid, &p);
        }
        let levels = p.levels_for_ui();
        self.dpi_rows = levels.0;
        self.dpi_def = levels.1;
        self.dpi_shift = levels.2;
        self.sync_zones = p.led(0) == p.led(1);
        self.profile = Some(p);
        self.dirty = false;
        self.save_cfg_now();
    }

    fn handle_events(&mut self) {
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Event::Connected(info, p, st) => {
                    self.sel_button = self.sel_button.min(info.button_count.saturating_sub(1));
                    self.conn = Conn::Ready(info);
                    self.status = st;
                    self.load_profile(p);
                }
                Event::NoDevice(msg) => {
                    if matches!(self.conn, Conn::Ready(_)) {
                        self.flash_err("Mouse desconectado");
                    }
                    self.conn = Conn::Missing(msg);
                    self.profile = None;
                    self.status = None;
                    self.dirty = false;
                    self.busy = None;
                }
                Event::Profile(p) => {
                    self.busy = None;
                    self.load_profile(p);
                    self.flash("Configuração lida do mouse");
                }
                Event::Status(s) => self.status = Some(s),
                Event::Applied(p) => {
                    self.busy = None;
                    self.load_profile(p);
                    let name = self.device().map(|d| d.name.clone()).unwrap_or_default();
                    if self.uses_macros() && !agent::is_running() {
                        match agent::start_detached() {
                            Ok(()) => self.flash(format!("Perfil gravado no {name} · agente de macros iniciado")),
                            Err(e) => self.flash_err(format!("Perfil gravado, mas o agente não iniciou: {e}")),
                        }
                    } else {
                        self.flash(format!("Perfil gravado no {name}"));
                    }
                }
                Event::Restored(path, p) => {
                    self.busy = None;
                    self.load_profile(p);
                    let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    self.flash(format!("Perfil original restaurado ({file})"));
                }
                Event::DpiTested(v) => {
                    if let Some(s) = &mut self.status {
                        s.dpi = v;
                    }
                }
                Event::Error(e) => {
                    self.busy = None;
                    self.flash_err(e);
                }
            }
        }
    }

    pub fn uses_macros(&self) -> bool {
        let (Some(d), Some(p)) = (self.device(), &self.profile) else { return false };
        (0..d.button_count).any(|i| [false, true].iter().any(|&g| self.cfg.macro_on_button(d.pid, p, i, g).is_some()))
    }

    // --- ações do cabeçalho ----------------------------------------------------------------

    fn do_reload(&mut self) {
        self.busy = Some("Lendo o mouse…");
        self.worker.send(Cmd::Reload);
    }

    fn do_apply(&mut self) {
        let Some(p) = self.profile.clone() else { return };
        if let Some(d) = self.device() {
            let pid = d.pid;
            self.cfg.prune(pid, &p);
        }
        self.save_cfg_now();
        self.busy = Some("Gravando no mouse…");
        self.worker.send(Cmd::Apply(p));
    }

    fn do_restore(&mut self) {
        self.busy = Some("Restaurando…");
        self.worker.send(Cmd::Restore);
    }

    pub fn set_binding(&mut self, button: usize, gshift: bool, b: dialogs::BindingChoice) {
        let (Some(d), Some(p)) = (self.device().cloned(), self.profile.as_mut()) else { return };
        match b {
            dialogs::BindingChoice::Plain(b) => p.set_button(button, b, gshift),
            dialogs::BindingChoice::Macro(id) => {
                if let Err(e) = self.cfg.assign_macro(d.pid, p, button, gshift, &id) {
                    self.flash_err(e.to_string());
                    return;
                }
                self.cfg_changed();
            }
        }
        self.set_dirty();
    }

    pub fn default_binding(i: usize, gshift: bool) -> Binding {
        if gshift {
            Binding::Disabled
        } else if i < 5 {
            Binding::Mouse(i as u8 + 1)
        } else {
            Binding::Special(0x05)
        }
    }

    // --- molduras --------------------------------------------------------------------------

    fn title_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter();
        p.rect_filled(rect, 0, TITLEBAR);
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, LINE));
        let drag = ui.interact(rect, ui.scope_id().with("titlebar"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.double_clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }
        let mut job = caps_job(&format!("{} {}", util::APP_NAME, util::VERSION), DIM);
        job.sections[0].format.font_id = mono(11.0);
        let g = ui.painter().layout_job(job);
        let icon = Rect::from_center_size(Pos2::new(rect.left() + 23.0, rect.center().y), Vec2::splat(14.0));
        ui.painter().image(self.logo.id(), icon, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        ui.painter().galley(Pos2::new(icon.right() + 9.0, rect.center().y - g.size().y / 2.0), g, DIM);

        let buttons = [("—", 0), (if maximized { "❐" } else { "▢" }, 1), ("✕", 2)];
        let mut x = rect.right() - 6.0 - 3.0 * 40.0 - 4.0;
        for (label, k) in buttons {
            let r = Rect::from_min_size(Pos2::new(x, rect.top() + 5.0), Vec2::new(40.0, 28.0));
            let resp = ui.interact(r, ui.scope_id().with(("wbtn", k)), Sense::click());
            let (bg, fg) = match (resp.hovered(), k) {
                (true, 2) => (hex(0xE5484D), Color32::WHITE),
                (true, _) => (hex(0x1B1D22), MUTED),
                _ => (Color32::TRANSPARENT, MUTED),
            };
            ui.painter().rect_filled(r, 6, bg);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, f(if k == 1 { 12.0 } else { 14.0 }), fg);
            if resp.clicked() {
                match k {
                    0 => ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true)),
                    1 => ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                    _ => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
                }
            }
            x += 42.0;
        }
    }

    /// Bordas invisíveis para redimensionar a janela sem moldura.
    fn resize_edges(&self, ui: &mut Ui, rect: Rect) {
        use egui::viewport::ResizeDirection as D;
        if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let t = 5.0;
        let zones = [
            (Rect::from_min_max(rect.left_top(), Pos2::new(rect.left() + t, rect.bottom())), D::West, egui::CursorIcon::ResizeWest),
            (Rect::from_min_max(Pos2::new(rect.right() - t, rect.top()), rect.right_bottom()), D::East, egui::CursorIcon::ResizeEast),
            (Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - t), rect.right_bottom()), D::South, egui::CursorIcon::ResizeSouth),
            (Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), rect.top() + 3.0)), D::North, egui::CursorIcon::ResizeNorth),
            (Rect::from_min_size(Pos2::new(rect.right() - 12.0, rect.bottom() - 12.0), Vec2::splat(12.0)), D::SouthEast, egui::CursorIcon::ResizeSouthEast),
            (Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - 12.0), Vec2::splat(12.0)), D::SouthWest, egui::CursorIcon::ResizeSouthWest),
        ];
        for (i, (r, dir, cursor)) in zones.into_iter().enumerate() {
            let resp = ui.interact(r, ui.scope_id().with(("resize", i)), Sense::drag()).on_hover_cursor(cursor);
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(rect, 0, SIDEBAR);
        p.line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, LINE));
        let inner = rect.shrink2(Vec2::new(16.0, 0.0));
        let mut y = rect.top() + 22.0;

        // logotipo
        let mut job = egui::text::LayoutJob::default();
        let mut fmt = egui::TextFormat::simple(fb(22.0), TEXT);
        fmt.extra_letter_spacing = -0.6;
        job.append("UN", 0.0, fmt.clone());
        fmt.color = ACCENT;
        job.append("HUB", 0.0, fmt);
        let g = ui.painter().layout_job(job);
        let gw = g.size();
        let logo = Rect::from_center_size(Pos2::new(inner.left() + 6.0 + 12.0, y + gw.y / 2.0), Vec2::splat(24.0));
        p.image(self.logo.id(), logo, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let tx = logo.right() + 10.0;
        p.galley(Pos2::new(tx, y), g, TEXT);
        p.text(Pos2::new(tx + gw.x + 8.0, y + gw.y - 6.0), Align2::LEFT_BOTTOM, "sem G HUB", mono(10.0), DIM);
        y += gw.y + 24.0;

        // cartão do dispositivo
        let card = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 128.0));
        p.rect(card, 12, CARD, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        let c = card.shrink(14.0);
        let (label, dotc, name) = match &self.conn {
            Conn::Ready(d) => ("CONECTADO", ACCENT, d.name.clone()),
            Conn::Searching => ("PROCURANDO", AMBER, "…".into()),
            Conn::Missing(_) => ("DESCONECTADO", hex(0x5C616B), "Nenhum mouse".into()),
        };
        paint_caps(ui, Pos2::new(c.left(), c.top() + 6.0), Align2::LEFT_CENTER, label, MUTED);
        dot(ui, Pos2::new(c.right() - 4.0, c.top() + 6.0), 3.5, dotc, matches!(self.conn, Conn::Ready(_)));
        paint_text(ui, Pos2::new(c.left(), c.top() + 22.0), Align2::LEFT_TOP, &name, fm(17.0), TEXT, c.width());
        let half = (c.width() - 8.0) / 2.0;
        let stats = [
            ("MODO", match self.status { Some(s) if s.onboard => "Onboard".to_string(), Some(_) => "Host".into(), None => "—".into() }, false),
            ("DPI", self.status.map(|s| s.dpi.to_string()).unwrap_or("—".into()), true),
        ];
        for (i, (k, v, is_mono)) in stats.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(c.left() + i as f32 * (half + 8.0), c.top() + 54.0), Vec2::new(half, 46.0));
            p.rect_filled(r, 8, SIDEBAR);
            p.text(Pos2::new(r.left() + 10.0, r.top() + 8.0), Align2::LEFT_TOP, *k, mono(10.0), DIM);
            p.text(Pos2::new(r.left() + 10.0, r.bottom() - 8.0), Align2::LEFT_BOTTOM, v, if *is_mono { mono(13.0) } else { fm(13.0) }, TEXT);
        }
        if let Conn::Missing(msg) = &self.conn {
            let resp = ui.interact(card, ui.scope_id().with("devcard"), Sense::click()).on_hover_text(msg.clone());
            if resp.clicked() {
                self.conn = Conn::Searching;
                self.worker.send(Cmd::Reconnect);
            }
        }
        y = card.bottom() + 24.0;

        // navegação
        for (i, (tab, label, _)) in TABS.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 40.0));
            let resp = ui.interact(r, ui.scope_id().with(("nav", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let active = self.tab == *tab;
            if active {
                p.rect_filled(r, 9, CHIP);
            } else if resp.hovered() {
                p.rect_filled(r, 9, HOVER);
            }
            p.text(Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, format!("0{}", i + 1), mono(10.0),
                if active { ACCENT } else { FAINT });
            p.text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, *label, fm(14.0),
                if active { TEXT } else { MUTED });
            if resp.clicked() && !active {
                self.stop_captures();
                self.tab = *tab;
            }
            y += 42.0;
        }

        // agente de macros (rodapé)
        let foot_top = rect.bottom() - 16.0 - 104.0;
        p.line_segment([Pos2::new(inner.left(), foot_top), Pos2::new(inner.right(), foot_top)], Stroke::new(1.0, LINE));
        let fx = inner.left() + 6.0;
        let fw = inner.width() - 12.0;
        paint_caps(ui, Pos2::new(fx, foot_top + 16.0), Align2::LEFT_TOP, "AGENTE DE MACROS", DIM);
        let row = foot_top + 50.0;
        let (lbl, dc) = if self.agent_running {
            ("Em execução", ACCENT)
        } else if self.uses_macros() {
            ("Parado", AMBER)
        } else {
            ("Parado", DIM)
        };
        dot(ui, Pos2::new(fx + 3.5, row), 3.5, dc, false);
        let lr = paint_text(ui, Pos2::new(fx + 15.0, row), Align2::LEFT_CENTER, lbl, f(13.0), TEXT, fw - 80.0);
        if !self.agent_running && self.uses_macros() {
            ui.interact(lr, ui.scope_id().with("agenthint"), Sense::hover())
                .on_hover_text("Há botões com macro: sem o agente eles não fazem nada.");
        }
        let btxt = if self.agent_running { "Parar" } else { "Iniciar" };
        let bw = text_size(ui, btxt, f(12.0)).x + 18.0;
        let br = Rect::from_min_size(Pos2::new(fx + fw - bw, row - 13.0), Vec2::new(bw, 26.0));
        let bresp = ui.interact(br, ui.scope_id().with("agentbtn"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        p.rect(br, 6, Color32::TRANSPARENT, Stroke::new(1.0, if bresp.hovered() { BORDER3 } else { BORDER2 }), egui::StrokeKind::Inside);
        p.text(br.center(), Align2::CENTER_CENTER, btxt, f(12.0), if bresp.hovered() { TEXT } else { MUTED });
        if bresp.clicked() {
            if self.agent_running {
                agent::request_stop();
                self.flash("Agente de macros parado");
            } else {
                match agent::start_detached() {
                    Ok(()) => self.flash("Agente de macros iniciado (ícone na bandeja)"),
                    Err(e) => self.flash_err(format!("Não foi possível iniciar o agente: {e}")),
                }
            }
            self.last_poll = Instant::now() - Duration::from_millis(1600);
        }
        let srow = foot_top + 88.0;
        let sr = Rect::from_min_max(Pos2::new(fx, srow - 10.0), Pos2::new(fx + fw, srow + 10.0));
        let sresp = ui.interact(sr, ui.scope_id().with("autostart"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        p.text(Pos2::new(fx, srow), Align2::LEFT_CENTER, "Iniciar com o Windows", f(13.0), MUTED);
        paint_toggle(ui, Rect::from_center_size(Pos2::new(fx + fw - 15.0, srow), Vec2::new(30.0, 18.0)), self.autostart, sresp.hovered());
        if sresp.clicked() {
            match agent::set_autostart(!self.autostart) {
                Ok(()) => self.autostart = agent::autostart_enabled(),
                Err(e) => self.flash_err(e.to_string()),
            }
        }
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, LINE));
        let (_, title, sub) = TABS.iter().find(|t| t.0 == self.tab).unwrap();
        let mut tj = egui::text::LayoutJob::single_section(title.to_string(), egui::TextFormat::simple(fm(21.0), TEXT));
        tj.sections[0].format.extra_letter_spacing = -0.3;
        let g = ui.painter().layout_job(tj);
        p.galley(Pos2::new(rect.left() + 28.0, rect.center().y - 24.0), g, TEXT);
        p.text(Pos2::new(rect.left() + 28.0, rect.center().y + 6.0), Align2::LEFT_TOP, *sub, f(13.0), MUTED);

        // botões à direita
        let ready = self.profile.is_some() && self.busy.is_none();
        let mut right = rect.right() - 28.0;
        let mut place = |ui: &mut Ui, label: &str, kind: Btn, enabled: bool| -> bool {
            let font = if kind == Btn::Primary { fb(13.0) } else { f(13.0) };
            let w = text_size(ui, label, font).x + if kind == Btn::Primary { 32.0 } else { 28.0 };
            let r = Rect::from_min_size(Pos2::new(right - w, rect.center().y - 17.0), Vec2::new(w, 34.0));
            right -= w + 8.0;
            let mut c = child(ui, r);
            if !enabled {
                c.disable();
                c.set_opacity(0.4);
            }
            button_sized(&mut c, label, kind, Some(w), 13.0).clicked()
        };
        if place(ui, "Aplicar no mouse", Btn::Primary, ready) {
            if self.ghub {
                self.dialog = Some(Dialog::confirm(
                    "G HUB aberto",
                    "O G HUB está aberto e vai sobrepor esta configuração. Feche-o (inclusive na bandeja) ou grave mesmo assim.",
                    "Gravar mesmo assim",
                    false,
                    dialogs::Pending::Apply,
                ));
            } else {
                self.do_apply();
            }
        }
        if place(ui, "Restaurar original", Btn::Outline, ready) {
            self.dialog = Some(Dialog::confirm(
                "Restaurar original",
                "Voltar ao perfil que estava no mouse antes do primeiro uso do UNHUB (ou do LGLITE)? Isso grava direto no mouse.",
                "Restaurar",
                true,
                dialogs::Pending::Restore,
            ));
        }
        if place(ui, "Recarregar do mouse", Btn::Outline, ready) {
            if self.dirty {
                self.dialog = Some(Dialog::confirm("Recarregar", "Descartar as alterações não aplicadas?", "Descartar", true, dialogs::Pending::Reload));
            } else {
                self.do_reload();
            }
        }
        if self.dirty || self.busy.is_some() {
            let (txt, col) = match self.busy {
                Some(b) => (b, BLUE),
                None => ("não aplicado", AMBER),
            };
            let tw = text_size(ui, txt, mono(12.0)).x;
            let x = right - 8.0 - tw;
            dot(ui, Pos2::new(x - 9.0, rect.center().y), 3.0, col, false);
            p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, txt, mono(12.0), col);
        }
    }

    fn empty_state(&mut self, ui: &mut Ui, rect: Rect) {
        let (title, body) = match &self.conn {
            Conn::Searching => ("Procurando o mouse…", "Conecte um mouse Logitech G com fio.".to_string()),
            Conn::Missing(m) => ("Nenhum mouse encontrado", format!("{m}.\nConecte o mouse — o UNHUB tenta de novo sozinho.")),
            Conn::Ready(_) => ("Lendo o perfil…", String::new()),
        };
        let c = rect.center();
        ui.painter().text(Pos2::new(c.x, c.y - 20.0), Align2::CENTER_CENTER, title, fm(18.0), TEXT);
        let g = ui.painter().layout(body, f(13.0), MUTED, 520.0);
        let w = g.size().x;
        ui.painter().galley(Pos2::new(c.x - w / 2.0, c.y + 4.0), g, MUTED);
    }

    fn content(&mut self, ui: &mut Ui, rect: Rect) {
        let inner = Rect::from_min_max(rect.min + Vec2::new(28.0, 24.0), rect.max - Vec2::new(28.0, 24.0));
        let mut inner = inner;
        if self.ghub || self.legacy_agent && self.uses_macros() {
            let msg = if self.ghub {
                "O G HUB está aberto: ele assume o controle do mouse e ignora o perfil onboard. Feche-o (inclusive na bandeja)."
            } else {
                "O agente do LGLITE (Python) está aberto junto: feche-o pelo ícone da bandeja para não tocar macros em dobro."
            };
            let r = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 38.0));
            ui.painter().rect(r, 10, alpha(AMBER, 0.08), Stroke::new(1.0, alpha(AMBER, 0.35)), egui::StrokeKind::Inside);
            dot(ui, Pos2::new(r.left() + 16.0, r.center().y), 3.0, AMBER, false);
            paint_text(ui, Pos2::new(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, msg, f(13.0), AMBER, r.width() - 40.0);
            inner.min.y += 50.0;
        }
        if self.profile.is_none() && self.tab != Tab::Macros {
            self.empty_state(ui, inner);
            return;
        }
        match self.tab {
            Tab::Buttons => self.buttons_page(ui, inner),
            Tab::Macros => self.macros_page(ui, inner),
            Tab::Dpi => self.dpi_page(ui, inner),
            Tab::Light => self.light_page(ui, inner),
        }
    }

    fn paint_toast(&mut self, ui: &mut Ui, area: Rect) {
        let Some((msg, at, err)) = &self.toast else { return };
        let age = at.elapsed().as_secs_f32();
        let life = if *err { 5.0 } else { 2.6 };
        if age > life {
            self.toast = None;
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        let a = (age / 0.15).min(1.0) * ((life - age) / 0.3).min(1.0);
        let g = ui.painter().layout(msg.clone(), fm(13.0), SIDEBAR, 640.0);
        let size = g.size() + Vec2::new(32.0, 20.0);
        let r = Rect::from_center_size(Pos2::new(area.center().x, area.bottom() - 20.0 - size.y / 2.0 + (1.0 - a) * 8.0), size);
        let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("toast")));
        for i in 1..=6 {
            painter.rect_filled(r.expand(i as f32 * 2.5).translate(Vec2::new(0.0, 6.0)), 12, alpha(Color32::BLACK, 0.06 * a));
        }
        let bg = if *err { hex(0xFFD9D4) } else { TEXT };
        painter.rect_filled(r, 9, alpha(bg, a));
        painter.galley(r.min + Vec2::new(16.0, 10.0), g, alpha(SIDEBAR, a));
    }

    fn poll_system(&mut self) {
        if self.last_poll.elapsed() >= Duration::from_millis(1500) {
            self.last_poll = Instant::now();
            self.agent_running = agent::is_running();
            self.legacy_agent = agent::legacy_running();
            self.ghub = util::ghub_running();
        }
        if self.cfg_dirty && self.cfg_changed_at.elapsed() >= Duration::from_millis(400) {
            self.save_cfg_now();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events();
        self.poll_system();
        self.poll_captures(&ctx);
        ctx.request_repaint_after(Duration::from_millis(1000));

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.dirty {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.dialog = Some(Dialog::confirm("Sair", "Sair sem aplicar as alterações no mouse?", "Sair", true, dialogs::Pending::Close));
            } else {
                self.stop_captures();
                if self.cfg_dirty {
                    self.save_cfg_now();
                }
            }
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, BG);
        let title = Rect::from_min_size(full.min, Vec2::new(full.width(), 38.0));
        let side = Rect::from_min_max(Pos2::new(full.left(), title.bottom()), Pos2::new(full.left() + 240.0, full.bottom()));
        let main = Rect::from_min_max(Pos2::new(side.right(), title.bottom()), full.max);
        let header = Rect::from_min_size(main.min, Vec2::new(main.width(), 76.0));
        let content = Rect::from_min_max(Pos2::new(main.left(), header.bottom()), main.max);

        self.title_bar(ui, title);
        self.sidebar(ui, side);
        self.header(ui, header);
        {
            let mut c = child(ui, content);
            c.set_clip_rect(content);
            self.content(&mut c, content);
        }
        self.paint_toast(ui, content);
        self.show_dialog(&ctx);
        self.resize_edges(ui, full);
        ui.painter().rect_stroke(full, 0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    }
}

impl Profile {
    /// Níveis de DPI como as 5 linhas da interface: (ligado, valor), índice padrão e shift.
    fn levels_for_ui(&self) -> ([(bool, u32); DPI_SLOTS], usize, usize) {
        let lv = self.dpi_levels();
        let mut rows = [(false, 800); DPI_SLOTS];
        for (i, v) in lv.iter().enumerate() {
            rows[i] = (*v != 0, if *v != 0 { *v } else { 800 });
        }
        (rows, self.default_dpi_index(), self.shift_dpi_index())
    }
}
