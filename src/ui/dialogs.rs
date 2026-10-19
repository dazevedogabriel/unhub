//! Janelas modais: confirmações e edição de ações de macro.

use eframe::egui::{self, Align2, Color32, Margin, Stroke, Ui, Vec2, ViewportCommand};

use super::theme::*;
use super::App;
use crate::capture::{Capture, ComboBuilder};
use crate::keys;
use crate::macros::{Step, WheelDir};
use crate::onboard::{mouse_button_name, Binding};

pub enum BindingChoice {
    Plain(Binding),
    Macro(String),
}

pub enum Pending {
    Reload,
    Restore,
    Apply,
    Close,
    DeleteMacro(String),
    SetBinding(usize, bool, BindingChoice),
}

#[derive(Clone, Copy, PartialEq)]
pub enum StepKind {
    Key,
    Text,
    Click,
    Wheel,
    Delay,
    Media,
    Launch,
}

impl StepKind {
    pub const ADD: [(StepKind, &'static str); 7] = [
        (StepKind::Key, "Tecla"), (StepKind::Text, "Texto"), (StepKind::Click, "Clique"), (StepKind::Wheel, "Rolagem"),
        (StepKind::Delay, "Atraso"), (StepKind::Media, "Mídia"), (StepKind::Launch, "Programa"),
    ];
}

pub struct StepDialog {
    pub kind: StepKind,
    /// índice do passo em edição (None = adicionar)
    pub editing: Option<usize>,
    pub cap: Option<Capture>,
    pub combo: ComboBuilder,
    pub single_key: u16,
    pub down: bool,
    pub text: String,
    pub button: u8,
    pub dir: WheelDir,
    pub amount: u32,
    pub ms: u32,
    pub media: u16,
    pub path: String,
    pub args: String,
}

impl StepDialog {
    pub fn new(kind: StepKind, default_delay: u32) -> Self {
        Self {
            kind,
            editing: None,
            cap: None,
            combo: ComboBuilder::default(),
            single_key: 0,
            down: true,
            text: String::new(),
            button: 1,
            dir: WheelDir::Down,
            amount: 3,
            ms: default_delay.max(1),
            media: keys::CONSUMER[0].0,
            path: String::new(),
            args: String::new(),
        }
    }

    pub fn edit(i: usize, s: &Step) -> Self {
        let mut d = Self::new(StepKind::Delay, 50);
        d.editing = Some(i);
        match s.clone() {
            Step::Key { code, down } => {
                d.kind = StepKind::Key;
                d.single_key = code;
                d.down = down;
            }
            Step::Mouse { button, down } => {
                d.kind = StepKind::Click;
                d.button = button;
                d.down = down;
            }
            Step::Wheel { dir, amount } => {
                d.kind = StepKind::Wheel;
                d.dir = dir;
                d.amount = amount;
            }
            Step::Delay { ms } => d.ms = ms,
            Step::Text { text } => {
                d.kind = StepKind::Text;
                d.text = text;
            }
            Step::Media { code } => {
                d.kind = StepKind::Media;
                d.media = code;
            }
            Step::Launch { path, args } => {
                d.kind = StepKind::Launch;
                d.path = path;
                d.args = args;
            }
        }
        d
    }

    fn title(&self) -> &'static str {
        let edit = self.editing.is_some();
        match self.kind {
            StepKind::Key => if edit { "Trocar tecla" } else { "Adicionar tecla" },
            StepKind::Text => "Texto a digitar",
            StepKind::Click => "Clique do mouse",
            StepKind::Wheel => "Rolagem",
            StepKind::Delay => "Atraso",
            StepKind::Media => "Mídia",
            StepKind::Launch => "Abrir programa",
        }
    }

    /// Passos resultantes, ou None se ainda incompleto.
    fn result(&self) -> Option<Vec<Step>> {
        Some(match self.kind {
            StepKind::Key => {
                if let Some(_) = self.editing {
                    if self.single_key == 0 {
                        return None;
                    }
                    vec![Step::Key { code: self.single_key, down: self.down }]
                } else {
                    let (mods, key) = (self.combo.mods, self.combo.key);
                    if mods == 0 && key == 0 {
                        return None;
                    }
                    let codes: Vec<u16> = keys::MODIFIERS.iter().enumerate()
                        .filter(|(i, _)| mods & (1 << i) != 0).map(|(_, (c, _))| *c as u16).collect();
                    let mut v: Vec<Step> = codes.iter().map(|&c| Step::Key { code: c, down: true }).collect();
                    if key != 0 {
                        v.push(Step::Key { code: key as u16, down: true });
                        v.push(Step::Key { code: key as u16, down: false });
                    }
                    v.extend(codes.iter().rev().map(|&c| Step::Key { code: c, down: false }));
                    v
                }
            }
            StepKind::Text => {
                if self.text.is_empty() {
                    return None;
                }
                vec![Step::Text { text: self.text.clone() }]
            }
            StepKind::Click => {
                if self.editing.is_some() {
                    vec![Step::Mouse { button: self.button, down: self.down }]
                } else {
                    vec![Step::Mouse { button: self.button, down: true }, Step::Mouse { button: self.button, down: false }]
                }
            }
            StepKind::Wheel => vec![Step::Wheel { dir: self.dir, amount: self.amount.clamp(1, 100) }],
            StepKind::Delay => vec![Step::Delay { ms: self.ms.min(600_000) }],
            StepKind::Media => vec![Step::Media { code: self.media }],
            StepKind::Launch => {
                if self.path.trim().is_empty() {
                    return None;
                }
                vec![Step::Launch { path: self.path.trim().into(), args: self.args.trim().into() }]
            }
        })
    }
}

pub enum Dialog {
    Confirm { title: String, body: String, ok: String, danger: bool, action: Pending },
    Step(StepDialog),
    Info { title: String, body: String },
}

impl Dialog {
    pub fn confirm(title: &str, body: &str, ok: &str, danger: bool, action: Pending) -> Self {
        Dialog::Confirm { title: title.into(), body: body.into(), ok: ok.into(), danger, action }
    }
}

enum Outcome {
    None,
    Cancel,
    Confirm,
}

fn modal_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(14)
        .inner_margin(Margin::same(22))
        .shadow(egui::Shadow { offset: [0, 24], blur: 64, spread: 0, color: alpha(Color32::BLACK, 0.6) })
}

fn footer(ui: &mut Ui, ok: &str, danger: bool, ok_enabled: bool) -> Outcome {
    let mut out = Outcome::None;
    ui.add_space(14.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap()).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if !ok_enabled {
            c.disable();
        }
        let resp = button(&mut c, ok, if danger { Btn::Danger } else { Btn::Primary });
        if resp.clicked() {
            out = Outcome::Confirm;
        }
        let w = resp.rect.width() + 8.0;
        ui.add_space(w);
        if button(ui, "Cancelar", Btn::Outline).clicked() {
            out = Outcome::Cancel;
        }
    });
    out
}

impl App {
    pub fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let modal = egui::Modal::new(egui::Id::unique("unhub-modal"))
            .frame(modal_frame())
            .backdrop_color(alpha(Color32::BLACK, 0.55));
        let mut outcome = Outcome::None;
        let resp = modal.show(ctx, |ui| {
            ui.set_width(440.0);
            match &mut dialog {
                Dialog::Confirm { title, body, ok, danger, .. } => {
                    ui.label(egui::RichText::new(title.as_str()).font(fm(17.0)).color(TEXT));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(body.as_str()).font(f(13.0)).color(MUTED));
                    outcome = footer(ui, ok, *danger, true);
                }
                Dialog::Info { title, body } => {
                    ui.label(egui::RichText::new(title.as_str()).font(fm(17.0)).color(TEXT));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(body.as_str()).font(f(13.0)).color(MUTED));
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if button(ui, "OK", Btn::Primary).clicked() {
                            outcome = Outcome::Cancel;
                        }
                    });
                }
                Dialog::Step(d) => {
                    ui.label(egui::RichText::new(d.title()).font(fm(17.0)).color(TEXT));
                    ui.add_space(10.0);
                    step_body(ui, d, ctx);
                    let ok = if d.editing.is_some() { "Salvar" } else { "Adicionar" };
                    outcome = footer(ui, ok, false, d.result().is_some());
                }
            }
        });
        // Esc só fecha se não estivermos capturando teclas (Esc pode ser a tecla desejada)
        let capturing = matches!(&dialog, Dialog::Step(d) if d.cap.is_some());
        if matches!(outcome, Outcome::None) && !capturing && resp.should_close() {
            outcome = Outcome::Cancel;
        }
        match outcome {
            Outcome::None => self.dialog = Some(dialog),
            Outcome::Cancel => {}
            Outcome::Confirm => match dialog {
                Dialog::Confirm { action, .. } => self.run_pending(action, ctx),
                Dialog::Step(d) => {
                    if let Some(steps) = d.result() {
                        self.insert_steps(d.editing, steps);
                    }
                }
                Dialog::Info { .. } => {}
            },
        }
    }

    fn run_pending(&mut self, action: Pending, ctx: &egui::Context) {
        match action {
            Pending::Reload => self.do_reload(),
            Pending::Restore => self.do_restore(),
            Pending::Apply => self.do_apply(),
            Pending::Close => {
                self.allow_close = true;
                self.stop_captures();
                if self.cfg_dirty {
                    self.save_cfg_now();
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Pending::DeleteMacro(id) => self.delete_macro(&id),
            Pending::SetBinding(i, g, b) => self.set_binding(i, g, b),
        }
    }

    /// Lê os eventos de teclado capturados (atalho do botão, gravação, diálogo de tecla).
    pub fn poll_captures(&mut self, _ctx: &egui::Context) {
        if let Some((cap, combo)) = &mut self.shortcut_cap {
            let mut done = None;
            for e in cap.drain() {
                if combo.feed(e) {
                    done = Some((combo.mods, combo.key));
                    break;
                }
            }
            if let Some((mods, key)) = done {
                self.shortcut_cap = None;
                let b = Binding::Key { key, mods };
                self.choose_binding(BindingChoice::Plain(b));
            }
        }
        if let Some(Dialog::Step(d)) = &mut self.dialog {
            if let Some(cap) = &d.cap {
                let evs = cap.drain();
                let mut stop = false;
                for e in evs {
                    if d.editing.is_some() {
                        if e.down {
                            d.single_key = e.hid;
                            stop = true;
                        }
                    } else if d.combo.feed(e) {
                        stop = true;
                    }
                    if stop {
                        break;
                    }
                }
                if stop {
                    d.cap = None;
                }
            }
        }
        self.poll_record();
    }
}

fn label(ui: &mut Ui, t: &str) {
    ui.label(caps_job(t, DIM));
}

fn capture_box(ui: &mut Ui, text: &str, active: bool, hint: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), egui::Sense::click());
    ui.painter().rect(rect, 10, INPUT, Stroke::new(1.0, if active { ACCENT } else if resp.hovered() { BORDER3 } else { BORDER2 }),
        egui::StrokeKind::Inside);
    if active {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, hint, fm(14.0), ACCENT);
    } else {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, fm(15.0), TEXT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn step_body(ui: &mut Ui, d: &mut StepDialog, ctx: &egui::Context) {
    ui.spacing_mut().item_spacing = Vec2::new(6.0, 8.0);
    match d.kind {
        StepKind::Key => {
            let text = if d.editing.is_some() {
                if d.single_key == 0 { "(nenhuma)".into() } else { keys::key_name(d.single_key) }
            } else {
                keys::describe_keystroke(d.combo.mods, d.combo.key)
            };
            let active = d.cap.is_some();
            let resp = capture_box(ui, &text, active, "Pressione a tecla…");
            if resp.clicked() {
                if d.cap.is_none() {
                    d.combo = ComboBuilder::default();
                    d.cap = Capture::start(ctx.clone()).ok();
                } else {
                    d.cap = None;
                }
            }
            let hint = if d.editing.is_some() {
                "Clique na caixa e pressione a nova tecla. O sentido (pressionar/soltar) é mantido."
            } else {
                "Clique na caixa e pressione a tecla ou combinação. Adiciona pressionar e soltar; modificadores envolvem a tecla."
            };
            ui.label(egui::RichText::new(hint).font(f(12.0)).color(DIM));
            if d.editing.is_some() {
                ui.horizontal(|ui| {
                    if chip(ui, "↓ Pressionar", d.down).clicked() { d.down = true; }
                    if chip(ui, "↑ Soltar", !d.down).clicked() { d.down = false; }
                });
            }
        }
        StepKind::Text => {
            label(ui, "TEXTO");
            ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(4).desired_width(f32::INFINITY)
                .font(f(14.0)).frame(input_frame()));
            ui.label(egui::RichText::new("Digitado como Unicode — funciona com acentos e emoji. Quebras de linha viram Enter.").font(f(12.0)).color(DIM));
        }
        StepKind::Click => {
            label(ui, "BOTÃO");
            ui.horizontal_wrapped(|ui| {
                for b in 1..=5u8 {
                    if chip(ui, &mouse_button_name(b), d.button == b).clicked() {
                        d.button = b;
                    }
                }
            });
            if d.editing.is_some() {
                label(ui, "SENTIDO");
                ui.horizontal(|ui| {
                    if chip(ui, "↓ Pressionar", d.down).clicked() { d.down = true; }
                    if chip(ui, "↑ Soltar", !d.down).clicked() { d.down = false; }
                });
            } else {
                ui.label(egui::RichText::new("Adiciona pressionar e soltar.").font(f(12.0)).color(DIM));
            }
        }
        StepKind::Wheel => {
            label(ui, "DIREÇÃO");
            ui.horizontal_wrapped(|ui| {
                for dir in WheelDir::ALL {
                    if chip(ui, dir.label(), d.dir == dir).clicked() {
                        d.dir = dir;
                    }
                }
            });
            label(ui, "PASSOS");
            num_edit(ui, "amount", &mut d.amount, 1, 100, 1, 120.0, None);
        }
        StepKind::Delay => {
            label(ui, "DURAÇÃO");
            num_edit(ui, "ms", &mut d.ms, 0, 600_000, 1, 160.0, Some("ms"));
            ui.horizontal_wrapped(|ui| {
                for v in [10u32, 25, 50, 100, 250, 500, 1000] {
                    if chip(ui, &format!("{v} ms"), d.ms == v).clicked() {
                        d.ms = v;
                    }
                }
            });
        }
        StepKind::Media => {
            label(ui, "AÇÃO");
            ui.horizontal_wrapped(|ui| {
                for (code, name, _) in keys::CONSUMER {
                    if chip(ui, name, d.media == code).clicked() {
                        d.media = code;
                    }
                }
            });
        }
        StepKind::Launch => {
            label(ui, "PROGRAMA");
            ui.horizontal(|ui| {
                let w = ui.available_width() - 96.0;
                ui.add_sized(Vec2::new(w, 34.0), egui::TextEdit::singleline(&mut d.path).font(f(13.0)).frame(input_frame())
                    .hint_text("C:\\caminho\\programa.exe"));
                if button(ui, "Procurar…", Btn::Outline).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Programas", &["exe", "bat", "cmd", "lnk"])
                        .add_filter("Todos", &["*"])
                        .pick_file()
                    {
                        d.path = p.display().to_string();
                    }
                }
            });
            label(ui, "ARGUMENTOS (OPCIONAL)");
            ui.add(egui::TextEdit::singleline(&mut d.args).font(f(13.0)).frame(input_frame()).desired_width(f32::INFINITY));
        }
    }
}
