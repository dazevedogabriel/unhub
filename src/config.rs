//! config.json em %APPDATA%\UNHUB: biblioteca de macros e gatilhos.
//!
//! Um botão com macro recebe no perfil onboard uma tecla sentinela (F13..F24, HID 0x68..0x73).
//! O agente captura essa tecla, engole e toca a macro associada.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::keys::SENTINELS;
use crate::macros::Macro;
use crate::onboard::{Binding, Profile};
use crate::util;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    #[serde(rename = "macro")]
    pub macro_id: String,
    pub button: usize,
    pub gshift: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// pid ("c08f") -> código HID sentinela ("104") -> gatilho
    #[serde(default)]
    pub triggers: BTreeMap<String, BTreeMap<String, Trigger>>,
    /// Código do idioma escolhido (p.ex. "en"); ausente = seguir o Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

pub fn path() -> PathBuf {
    util::data_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        let mut c: Config = std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for m in &mut c.macros {
            m.normalize();
        }
        c
    }

    pub fn save(&self) -> Result<()> {
        let p = path();
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    pub fn macro_by_id(&self, id: &str) -> Option<&Macro> {
        self.macros.iter().find(|m| m.id == id)
    }

    fn device_triggers(&mut self, pid: u16) -> &mut BTreeMap<String, Trigger> {
        self.triggers.entry(format!("{pid:04x}")).or_default()
    }

    /// Reaproveita a sentinela do botão ou reserva uma livre.
    fn sentinel_for(&mut self, pid: u16, profile: &Profile, button: usize, gshift: bool) -> Result<u8> {
        let trig = self.device_triggers(pid);
        if let Some((code, _)) = trig.iter().find(|(_, t)| t.button == button && t.gshift == gshift) {
            if let Ok(c) = code.parse() {
                return Ok(c);
            }
        }
        let mut used: HashSet<u8> = trig.keys().filter_map(|c| c.parse().ok()).collect();
        // não usar uma F13+ que o usuário pôs como atalho de verdade
        for i in 0..crate::onboard::BUTTON_SLOTS {
            for g in [false, true] {
                if let Binding::Key { key, .. } = profile.button(i, g) {
                    if SENTINELS.contains(&key) {
                        used.insert(key);
                    }
                }
            }
        }
        match SENTINELS.into_iter().find(|c| !used.contains(c)) {
            Some(c) => Ok(c),
            None => bail!(tr!("err.macro_limit")),
        }
    }

    pub fn assign_macro(&mut self, pid: u16, profile: &mut Profile, button: usize, gshift: bool, macro_id: &str) -> Result<()> {
        let code = self.sentinel_for(pid, profile, button, gshift)?;
        self.device_triggers(pid)
            .insert(code.to_string(), Trigger { macro_id: macro_id.into(), button, gshift });
        profile.set_button(button, Binding::Key { key: code, mods: 0 }, gshift);
        Ok(())
    }

    pub fn macro_on_button(&self, pid: u16, profile: &Profile, button: usize, gshift: bool) -> Option<&Macro> {
        let Binding::Key { key, mods: 0 } = profile.button(button, gshift) else { return None };
        if !SENTINELS.contains(&key) {
            return None;
        }
        let t = self.triggers.get(&format!("{pid:04x}"))?.get(&key.to_string())?;
        if t.button == button && t.gshift == gshift {
            self.macro_by_id(&t.macro_id)
        } else {
            None
        }
    }

    /// Remove gatilhos cujo botão não aponta mais para a sentinela (ou cuja macro sumiu).
    pub fn prune(&mut self, pid: u16, profile: &Profile) {
        let ids: HashSet<String> = self.macros.iter().map(|m| m.id.clone()).collect();
        self.device_triggers(pid).retain(|code, t| {
            let Ok(code) = code.parse::<u8>() else { return false };
            let points = profile.button(t.button, t.gshift) == Binding::Key { key: code, mods: 0 };
            points && ids.contains(&t.macro_id)
        });
    }

    pub fn unique_name(&self, base: &str) -> String {
        let names: HashSet<&str> = self.macros.iter().map(|m| m.name.as_str()).collect();
        if !names.contains(base) {
            return base.into();
        }
        (2..).map(|n| format!("{base} {n}")).find(|n| !names.contains(n.as_str())).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::MacroType;

    const USER: &[u8] = include_bytes!("../tests/fixtures/g403hero_sector0001.bin");

    #[test]
    fn assign_reuse_and_prune() {
        let mut cfg = Config::default();
        let m = Macro::new("a", MacroType::Simple);
        let id = m.id.clone();
        cfg.macros.push(m);
        let mut p = Profile::new(USER.to_vec());
        cfg.assign_macro(0xC08F, &mut p, 4, false, &id).unwrap();
        assert_eq!(p.button(4, false), Binding::Key { key: 0x68, mods: 0 });
        assert_eq!(cfg.macro_on_button(0xC08F, &p, 4, false).unwrap().id, id);
        // mesmo botão reaproveita a sentinela; outro botão pega a próxima
        cfg.assign_macro(0xC08F, &mut p, 4, false, &id).unwrap();
        cfg.assign_macro(0xC08F, &mut p, 3, true, &id).unwrap();
        assert_eq!(p.button(3, true), Binding::Key { key: 0x69, mods: 0 });
        p.set_button(4, Binding::Mouse(5), false);
        cfg.prune(0xC08F, &p);
        assert_eq!(cfg.triggers["c08f"].len(), 1);
        assert!(cfg.macro_on_button(0xC08F, &p, 4, false).is_none());
    }

    #[test]
    fn reads_python_config() {
        let j = r#"{"macros":[{"id":"d2","name":"Rapid Left","type":"repeat","defaultDelay":50,"useDefaultDelay":false,
            "simple":[],"press":[],"held":[{"t":"mouse","button":1,"down":true}],"release":[],"toggle":[]}],
            "triggers":{"c08f":{"104":{"macro":"d2","button":4,"gshift":false}}}}"#;
        let c: Config = serde_json::from_str(j).unwrap();
        assert_eq!(c.triggers["c08f"]["104"].macro_id, "d2");
        assert_eq!(c.macros[0].held.len(), 1);
    }
}
