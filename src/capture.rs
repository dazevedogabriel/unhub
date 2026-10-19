//! Captura de teclas na interface (atalhos e gravação de macros).
//!
//! Usa um hook de teclado só enquanto captura: assim aparecem as teclas que o egui não
//! conhece (F13+, ABNT2, PrintScreen), com esquerda/direita distintos nos modificadores.
//! Só engole as teclas quando a janela do UNHUB está em primeiro plano.

use std::sync::{Arc, Mutex};

use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::keys;
use crate::winput::KeyboardHook;

#[derive(Clone, Copy, Debug)]
pub struct Captured {
    pub hid: u16,
    pub down: bool,
    pub time_ms: u32,
}

pub struct Capture {
    _hook: KeyboardHook,
    events: Arc<Mutex<Vec<Captured>>>,
}

fn we_are_foreground() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid == GetCurrentProcessId()
    }
}

impl Capture {
    pub fn start(ctx: eframe::egui::Context) -> anyhow::Result<Self> {
        let events: Arc<Mutex<Vec<Captured>>> = Arc::default();
        let ev = events.clone();
        let hook = KeyboardHook::start(move |k| {
            if !we_are_foreground() {
                return false;
            }
            if let Some(hid) = keys::vk_to_hid(k.vk, k.extended) {
                ev.lock().unwrap().push(Captured { hid, down: k.down, time_ms: k.time });
                ctx.request_repaint();
            }
            true
        })?;
        Ok(Self { _hook: hook, events })
    }

    pub fn drain(&self) -> Vec<Captured> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

/// Monta uma combinação (modificadores + tecla) a partir dos eventos, como uma caixa de atalho.
#[derive(Default)]
pub struct ComboBuilder {
    held_mods: u8,
    pub mods: u8,
    pub key: u8,
}

impl ComboBuilder {
    /// Retorna true quando a combinação está completa.
    pub fn feed(&mut self, e: Captured) -> bool {
        if let Some(bit) = keys::modifier_bit(e.hid) {
            if e.down {
                if self.held_mods == 0 {
                    self.mods = 0;
                    self.key = 0;
                }
                self.held_mods |= bit;
                self.mods |= bit;
            } else {
                self.held_mods &= !bit;
                // só modificadores (p.ex. "Win"): completa ao soltar tudo
                return self.held_mods == 0 && self.mods != 0;
            }
            false
        } else if e.down && e.hid <= 0xFF {
            if self.held_mods == 0 {
                self.mods = 0;
            }
            self.key = e.hid as u8;
            true
        } else {
            false
        }
    }
}
