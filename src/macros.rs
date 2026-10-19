//! Modelo de macros, no mesmo espírito do editor do G HUB.
//!
//! Tipos (MACRO_TYPE_* no app.asar do G HUB):
//!   simple    toca a lista `simple` uma vez ao pressionar
//!   repeat    repete a lista `held` enquanto o botão estiver pressionado
//!   toggle    1º toque começa a repetir `toggle`, 2º toque para
//!   sequence  `press` ao pressionar, `held` em loop enquanto segura, `release` ao soltar
//!
//! O formato JSON é o mesmo do LGLITE (Python), então config.json é compatível.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::keys;
use crate::onboard::mouse_button_name;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WheelDir {
    Up,
    Down,
    Left,
    Right,
}

impl WheelDir {
    pub const ALL: [WheelDir; 4] = [WheelDir::Up, WheelDir::Down, WheelDir::Left, WheelDir::Right];

    pub fn label(self) -> &'static str {
        match self {
            WheelDir::Up => "Rolar para cima",
            WheelDir::Down => "Rolar para baixo",
            WheelDir::Left => "Rolar para a esquerda",
            WheelDir::Right => "Rolar para a direita",
        }
    }
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Step {
    Key { code: u16, down: bool },
    Mouse { button: u8, down: bool },
    Wheel { dir: WheelDir, #[serde(default = "one")] amount: u32 },
    Delay { ms: u32 },
    Text { text: String },
    Media { code: u16 },
    Launch { path: String, #[serde(default)] args: String },
}

impl Step {
    pub fn valid(&self) -> bool {
        match self {
            Step::Mouse { button, .. } => (1..=5).contains(button),
            Step::Delay { ms } => *ms <= 600_000,
            Step::Launch { path, .. } => !path.is_empty(),
            Step::Wheel { amount, .. } => (1..=100).contains(amount),
            _ => true,
        }
    }

    /// (tag, texto) para a lista de ações.
    pub fn describe(&self) -> (&'static str, String) {
        match self {
            Step::Key { code, down } => (if *down { "DOWN" } else { "UP" }, format!("Tecla {}", keys::key_name(*code))),
            Step::Mouse { button, down } => (if *down { "DOWN" } else { "UP" }, mouse_button_name(*button)),
            Step::Wheel { dir, amount } => {
                ("SCROLL", if *amount != 1 { format!("{} · {amount}", dir.label()) } else { dir.label().into() })
            }
            Step::Delay { ms } => ("WAIT", format!("Aguardar {ms} ms")),
            Step::Text { text } => {
                let t: String = text.replace('\n', "⏎");
                let short: String = t.chars().take(48).collect();
                ("TEXT", format!("“{short}{}”", if t.chars().count() > 48 { "…" } else { "" }))
            }
            Step::Media { code } => ("MEDIA", keys::consumer_name(*code)),
            Step::Launch { path, args } => {
                ("RUN", if args.is_empty() { path.clone() } else { format!("{path} {args}") })
            }
        }
    }

    pub fn is_delay(&self) -> bool {
        matches!(self, Step::Delay { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroType {
    Simple,
    Repeat,
    Toggle,
    Sequence,
}

impl MacroType {
    /// Na ordem do seletor da interface.
    pub const ALL: [MacroType; 4] = [MacroType::Repeat, MacroType::Simple, MacroType::Toggle, MacroType::Sequence];

    pub fn label(self) -> &'static str {
        match self {
            MacroType::Repeat => "Repetir enquanto pressionado",
            MacroType::Simple => "Executar uma vez",
            MacroType::Toggle => "Alternar (liga/desliga)",
            MacroType::Sequence => "Pressionar e soltar",
        }
    }

    pub fn info(self) -> (&'static str, &'static str) {
        match self {
            MacroType::Repeat => ("Enquanto pressionado", "Repete as ações enquanto o botão estiver pressionado."),
            MacroType::Simple => ("Ao pressionar", "Executa a sequência uma vez a cada clique."),
            MacroType::Toggle => ("Em loop", "Um clique inicia o loop, outro clique para."),
            MacroType::Sequence => ("Ao pressionar / ao soltar", "Ações ao pressionar, em loop enquanto segura, e ao soltar."),
        }
    }

    pub fn lists(self) -> &'static [ListName] {
        match self {
            MacroType::Simple => &[ListName::Simple],
            MacroType::Repeat => &[ListName::Held],
            MacroType::Toggle => &[ListName::Toggle],
            MacroType::Sequence => &[ListName::Press, ListName::Held, ListName::Release],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListName {
    Simple,
    Press,
    Held,
    Release,
    Toggle,
}

impl ListName {
    pub fn label(self) -> &'static str {
        match self {
            ListName::Simple | ListName::Toggle => "Ações",
            ListName::Press => "Ao pressionar",
            ListName::Held => "Enquanto segura",
            ListName::Release => "Ao soltar",
        }
    }
}

fn lenient_steps<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Step>, D::Error> {
    let raw: Option<Vec<Value>> = Option::deserialize(d)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Step>(v).ok())
        .filter(Step::valid)
        .collect())
}

fn lenient_type<'de, D: Deserializer<'de>>(d: D) -> Result<MacroType, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or(MacroType::Simple))
}

fn default_delay() -> u32 {
    50
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    #[serde(default = "new_id")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default = "simple_type", deserialize_with = "lenient_type")]
    pub kind: MacroType,
    #[serde(rename = "defaultDelay", default = "default_delay")]
    pub default_delay: u32,
    #[serde(rename = "useDefaultDelay", default = "yes")]
    pub use_default_delay: bool,
    #[serde(default, deserialize_with = "lenient_steps")]
    pub simple: Vec<Step>,
    #[serde(default, deserialize_with = "lenient_steps")]
    pub press: Vec<Step>,
    #[serde(default, deserialize_with = "lenient_steps")]
    pub held: Vec<Step>,
    #[serde(default, deserialize_with = "lenient_steps")]
    pub release: Vec<Step>,
    #[serde(default, deserialize_with = "lenient_steps")]
    pub toggle: Vec<Step>,
}

fn simple_type() -> MacroType {
    MacroType::Simple
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Macro {
    pub fn new(name: &str, kind: MacroType) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            kind,
            default_delay: 50,
            use_default_delay: true,
            simple: vec![],
            press: vec![],
            held: vec![],
            release: vec![],
            toggle: vec![],
        }
    }

    pub fn list(&self, l: ListName) -> &Vec<Step> {
        match l {
            ListName::Simple => &self.simple,
            ListName::Press => &self.press,
            ListName::Held => &self.held,
            ListName::Release => &self.release,
            ListName::Toggle => &self.toggle,
        }
    }

    pub fn list_mut(&mut self, l: ListName) -> &mut Vec<Step> {
        match l {
            ListName::Simple => &mut self.simple,
            ListName::Press => &mut self.press,
            ListName::Held => &mut self.held,
            ListName::Release => &mut self.release,
            ListName::Toggle => &mut self.toggle,
        }
    }

    pub fn step_count(&self) -> usize {
        self.kind.lists().iter().map(|l| self.list(*l).len()).sum()
    }

    pub fn summary(&self) -> String {
        format!("{} · {} ações", self.kind.label(), self.step_count())
    }

    pub fn normalize(&mut self) {
        self.default_delay = self.default_delay.min(60_000);
        if self.name.trim().is_empty() {
            self.name = "Macro".into();
        }
    }

    /// Troca o tipo levando as ações junto quando a lista principal muda (p.ex. simples -> repetir).
    pub fn change_type(&mut self, new_type: MacroType) {
        if new_type == self.kind {
            return;
        }
        let old_lists = self.kind.lists();
        let new_lists = new_type.lists();
        let src = old_lists.iter().copied().find(|l| !self.list(*l).is_empty());
        let dst = if new_type != MacroType::Sequence {
            new_lists[0]
        } else if matches!(self.kind, MacroType::Repeat | MacroType::Toggle) {
            ListName::Held
        } else {
            ListName::Press
        };
        if let Some(src) = src {
            if self.list(dst).is_empty() && !new_lists.contains(&src) {
                let moved = std::mem::take(self.list_mut(src));
                *self.list_mut(dst) = moved;
            }
        }
        self.kind = new_type;
    }

    pub fn duplicate(&self) -> Self {
        Self { id: new_id(), name: format!("{} (cópia)", self.name), ..self.clone() }
    }
}

// --- importação do G HUB (settings.db) ------------------------------------------------

fn ghub_component(c: &Value, warnings: &mut Vec<String>) -> Vec<Step> {
    let int = |v: &Value| v.as_i64().unwrap_or(0);
    if let Some(k) = c.get("keyboard") {
        return vec![Step::Key { code: int(&k["hidUsage"]) as u16, down: k["isDown"].as_bool().unwrap_or(false) }];
    }
    if let Some(d) = c.get("delay") {
        return vec![Step::Delay { ms: int(&d["durationMs"]).clamp(0, 600_000) as u32 }];
    }
    if let Some(t) = c.get("textBlock") {
        return vec![Step::Text { text: t["text"].as_str().unwrap_or("").into() }];
    }
    if let Some(a) = c.get("app") {
        let args: Vec<&str> = a["argumentList"].as_array().map(|l| l.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        return vec![Step::Launch { path: a["executablePath"].as_str().unwrap_or("").into(), args: args.join(" ") }];
    }
    if let Some(m) = c.get("mouseMacro").or_else(|| c.get("mouse")) {
        if let Some(b) = m.get("button") {
            return vec![Step::Mouse { button: int(&b["hidUsage"]) as u8, down: b["isDown"].as_bool().unwrap_or(false) }];
        }
        if let Some(w) = m.get("wheel") {
            let vertical = w.get("hidUsage").map(int).unwrap_or(0x38) == 0x38;
            let pos = w["isPositive"].as_bool().unwrap_or(false);
            let dir = match (vertical, pos) {
                (true, true) => WheelDir::Up,
                (true, false) => WheelDir::Down,
                (false, true) => WheelDir::Right,
                (false, false) => WheelDir::Left,
            };
            return vec![Step::Wheel { dir, amount: 1 }];
        }
        if m["action"].as_str() == Some("BUTTON") {
            // forma "clique" (desce + sobe), p.ex. "Rapid Left"
            let b = m.get("hidUsage").map(int).unwrap_or(1) as u8;
            return vec![Step::Mouse { button: b, down: true }, Step::Mouse { button: b, down: false }];
        }
    }
    let keys: Vec<&String> = c.as_object().map(|o| o.keys().take(3).collect()).unwrap_or_default();
    warnings.push(format!("componente não suportado ignorado: {keys:?}"));
    vec![]
}

/// Card MACRO_PLAYBACK do G HUB -> macro (ou None se não for convertível).
pub fn from_ghub_card(card: &Value, warnings: &mut Vec<String>) -> Option<Macro> {
    let mac = card.get("macro")?;
    let name = card["name"].as_str().unwrap_or("Macro do G HUB");
    if mac["type"].as_str() == Some("KEYSTROKE") {
        let ks = &mac["keystroke"];
        let mods: Vec<u16> = ks["modifiers"].as_array().map(|l| l.iter().filter_map(|v| v.as_u64()).map(|v| v as u16).collect()).unwrap_or_default();
        let code = ks["code"].as_u64()? as u16;
        let mut steps: Vec<Step> = mods.iter().map(|&m| Step::Key { code: m, down: true }).collect();
        steps.push(Step::Key { code, down: true });
        steps.push(Step::Key { code, down: false });
        steps.extend(mods.iter().rev().map(|&m| Step::Key { code: m, down: false }));
        let mut m = Macro::new(name, MacroType::Simple);
        m.simple = steps;
        m.use_default_delay = false;
        return Some(m);
    }
    if mac["type"].as_str() != Some("SEQUENCE") {
        return None;
    }
    let seq = &mac["sequence"];
    let mut conv = |key: &str| -> Vec<Step> {
        let mut out = Vec::new();
        if let Some(cs) = seq.get(key).and_then(|s| s.get("components")).and_then(|c| c.as_array()) {
            for c in cs {
                out.extend(ghub_component(c, warnings));
            }
        }
        out.retain(Step::valid);
        out
    };
    let simple = conv("simpleSequence");
    let press = conv("pressSequence");
    let held = conv("heldSequence");
    let release = conv("releaseSequence");
    let toggle = conv("toggleSequence");
    let kind = if !press.is_empty() || !release.is_empty() {
        MacroType::Sequence
    } else if !toggle.is_empty() {
        MacroType::Toggle
    } else if !held.is_empty() {
        MacroType::Repeat
    } else {
        MacroType::Simple
    };
    let mut m = Macro::new(name, kind);
    m.simple = simple;
    m.press = press;
    m.held = held;
    m.release = release;
    m.toggle = toggle;
    m.default_delay = seq["defaultDelay"].as_u64().unwrap_or(50).min(60_000) as u32;
    m.use_default_delay = seq["useDefaultDelay"].as_bool().unwrap_or(true);
    Some(m)
}

pub fn ghub_settings_path() -> std::path::PathBuf {
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_default();
    local.join("LGHUB").join("settings.db")
}

/// Lê as macros criadas pelo usuário no settings.db do G HUB (somente leitura).
pub fn import_from_ghub(path: &std::path::Path) -> anyhow::Result<(Vec<Macro>, Vec<String>)> {
    use rusqlite::{Connection, OpenFlags};
    let con = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    let blob: Vec<u8> = con.query_row("select file from data order by _id desc limit 1", [], |r| {
        r.get::<_, Vec<u8>>(0).or_else(|_| r.get::<_, String>(0).map(String::into_bytes))
    })?;
    let data: Value = serde_json::from_slice(&blob)?;
    let mut warnings = Vec::new();
    let mut result = Vec::new();
    if let Some(cards) = data["cards"]["cards"].as_array() {
        for card in cards {
            // cards readOnly são presets embutidos do G HUB
            if card["attribute"].as_str() != Some("MACRO_PLAYBACK") || card["readOnly"].as_bool() == Some(true) {
                continue;
            }
            if let Some(m) = from_ghub_card(card, &mut warnings) {
                result.push(m);
            }
        }
    }
    Ok((result, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghub_rapid_left() {
        let card = serde_json::json!({"attribute": "MACRO_PLAYBACK", "id": "a85d6958", "name": "Rapid Left",
            "macro": {"type": "SEQUENCE", "sequence": {
                "defaultDelay": 50, "useDefaultDelay": true,
                "heldSequence": {"components": [{"mouseMacro": {"action": "BUTTON", "hidUsage": 1},
                                                  "name": "PRIMARY_CLICK", "type": "MOUSE"}]},
                "pressSequence": {}, "releaseSequence": {}, "simpleSequence": {}, "toggleSequence": {}}}});
        let mut w = vec![];
        let m = from_ghub_card(&card, &mut w).unwrap();
        assert_eq!(m.kind, MacroType::Repeat);
        assert_eq!(m.held, vec![Step::Mouse { button: 1, down: true }, Step::Mouse { button: 1, down: false }]);
        assert!(w.is_empty());
    }

    #[test]
    fn keystroke_card() {
        let card = serde_json::json!({"name": "Open", "macro": {"type": "KEYSTROKE", "keystroke": {"code": 18, "modifiers": [224]}}});
        let m = from_ghub_card(&card, &mut vec![]).unwrap();
        assert_eq!(m.simple.len(), 4);
        assert_eq!(m.simple[0], Step::Key { code: 224, down: true });
        assert_eq!(m.simple[3], Step::Key { code: 224, down: false });
    }

    #[test]
    fn lenient_load_drops_bad_steps() {
        let j = r#"{"id":"x","name":"a","type":"weird","held":[{"t":"mouse","button":9,"down":true},{"t":"delay","ms":5},{"t":"zzz"}]}"#;
        let m: Macro = serde_json::from_str(j).unwrap();
        assert_eq!(m.kind, MacroType::Simple);
        assert_eq!(m.held, vec![Step::Delay { ms: 5 }]);
        assert!(m.use_default_delay);
    }

    #[test]
    fn python_format_roundtrip() {
        let j = r#"{"id":"d2","name":"Rapid Left","type":"repeat","defaultDelay":50,"useDefaultDelay":false,
            "simple":[],"press":[],"held":[{"t":"mouse","button":1,"down":true},{"t":"wheel","dir":"up","amount":2},
            {"t":"launch","path":"notepad.exe","args":""}],"release":[],"toggle":[]}"#;
        let m: Macro = serde_json::from_str(j).unwrap();
        let back: Value = serde_json::to_value(&m).unwrap();
        assert_eq!(back["held"][0], serde_json::json!({"t":"mouse","button":1,"down":true}));
        assert_eq!(back["held"][1], serde_json::json!({"t":"wheel","dir":"up","amount":2}));
        assert_eq!(back["type"], "repeat");
    }

    #[test]
    fn change_type_moves_steps() {
        let mut m = Macro::new("x", MacroType::Simple);
        m.simple = vec![Step::Delay { ms: 1 }];
        m.change_type(MacroType::Repeat);
        assert_eq!(m.held.len(), 1);
        assert!(m.simple.is_empty());
        m.change_type(MacroType::Sequence);
        assert_eq!(m.held.len(), 1); // held já é lista da sequência
    }
}
