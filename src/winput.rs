//! Injeção (SendInput) e captura (WH_KEYBOARD_LL) de entrada no Windows.
//!
//! O agente do G HUB também injeta com SendInput. Aqui o teclado é enviado por scan code,
//! que é o que jogos com DirectInput/raw input leem.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::keys;
use crate::macros::WheelDir;

/// Marca nossos próprios eventos para os hooks ignorá-los ("LGLT", o mesmo do LGLITE).
pub const EXTRA_INFO: usize = 0x4C474C54;
const EXTENDED_VK: [u16; 18] = [0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2E, 0x6F, 0x90,
    0xA3, 0xA5, 0x5B, 0x5C, 0x5D, 0x2C];

fn send(inputs: &[INPUT]) -> Result<()> {
    let n = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    if n as usize != inputs.len() {
        return Err(anyhow!(tr!("err.sendinput", error = std::io::Error::last_os_error())));
    }
    Ok(())
}

fn kb(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: EXTRA_INFO } },
    }
}

fn ms(data: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: EXTRA_INFO } },
    }
}

fn key_input(vk: u16, down: bool, force_extended: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
    let mut flags = KEYEVENTF_SCANCODE | if down { 0 } else { KEYEVENTF_KEYUP };
    if force_extended || EXTENDED_VK.contains(&vk) || scan & 0xE000 == 0xE000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if scan == 0 {
        flags &= !KEYEVENTF_SCANCODE; // sem scan code (p.ex. teclas de mídia): envia por VK
    }
    kb(vk, (scan & 0xFF) as u16, flags)
}

pub fn key(hid: u16, down: bool) -> Result<()> {
    let vk = keys::hid_to_vk(hid).ok_or_else(|| anyhow!(tr!("err.key_unmapped", code = format!("{hid:02x}"))))?;
    send(&[key_input(vk, down, hid == 0x58)])
}

pub fn media(code: u16) -> Result<()> {
    let vk = keys::consumer_vk(code).ok_or_else(|| anyhow!(tr!("err.media_unsupported", code = format!("{code:04x}"))))?;
    send(&[kb(vk, 0, KEYEVENTF_EXTENDEDKEY), kb(vk, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP)])
}

pub fn text(s: &str) -> Result<()> {
    let mut inputs = Vec::new();
    for ch in s.replace("\r\n", "\n").chars() {
        if ch == '\n' {
            inputs.push(key_input(0x0D, true, false));
            inputs.push(key_input(0x0D, false, false));
            continue;
        }
        let mut buf = [0u16; 2];
        for &unit in ch.encode_utf16(&mut buf).iter() {
            inputs.push(kb(0, unit, KEYEVENTF_UNICODE));
            inputs.push(kb(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
    }
    if inputs.is_empty() { Ok(()) } else { send(&inputs) }
}

pub fn mouse_button(button: u8, down: bool) -> Result<()> {
    let (d, u, data) = match button {
        1 => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
        2 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        3 => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        4 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON1 as u32),
        5 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON2 as u32),
        _ => return Err(anyhow!(tr!("err.bad_button", n = button))),
    };
    send(&[ms(data, if down { d } else { u })])
}

pub fn wheel(dir: WheelDir, amount: u32) -> Result<()> {
    let horizontal = matches!(dir, WheelDir::Left | WheelDir::Right);
    let sign: i32 = if matches!(dir, WheelDir::Up | WheelDir::Right) { 1 } else { -1 };
    let delta = sign * WHEEL_DELTA as i32 * amount as i32;
    send(&[ms(delta as u32, if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL })])
}

// --- hook global de teclado ---------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub vk: u32,
    pub extended: bool,
    pub down: bool,
    pub time: u32,
}

/// `handler(ev) -> bool`: true engole a tecla. Precisa ser rápido (o Windows remove hooks lentos).
type Handler = Arc<dyn Fn(KeyEvent) -> bool + Send + Sync>;

static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        let ours = k.dwExtraInfo == EXTRA_INFO;
        let injected = k.flags & LLKHF_INJECTED != 0;
        if !ours && !injected {
            let handler = HANDLER.lock().ok().and_then(|h| h.clone());
            if let Some(h) = handler {
                let down = wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN;
                let ev = KeyEvent { vk: k.vkCode, extended: k.flags & LLKHF_EXTENDED != 0, down, time: k.time };
                if h(ev) {
                    return 1;
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Um hook por processo (o agente usa um; a interface usa outro só enquanto captura teclas).
pub struct KeyboardHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl KeyboardHook {
    pub fn start(handler: impl Fn(KeyEvent) -> bool + Send + Sync + 'static) -> Result<Self> {
        *HANDLER.lock().unwrap() = Some(Arc::new(handler));
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new().name("unhub-hook".into()).spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(std::ptr::null()), 0);
            let _ = tx.send((tid, !hook.is_null()));
            if hook.is_null() {
                return;
            }
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
            UnhookWindowsHookEx(hook);
        })?;
        let (thread_id, ok) = rx.recv()?;
        if !ok {
            return Err(anyhow!(tr!("err.hook")));
        }
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        *HANDLER.lock().unwrap() = None;
    }
}
