//! Agente de macros (ícone na bandeja).
//!
//! Captura as teclas sentinela F13..F24 que os botões com macro enviam, engole e toca a macro.
//! O mouse continua em modo onboard: DPI, cores e atalhos funcionam sem nenhum software.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{self, Config};
use crate::keys;
use crate::macros::Macro;
use crate::player::{Player, WinOutput};
use crate::util::wide;
use crate::winput::{KeyEvent, KeyboardHook};

const MUTEX_NAME: &str = "Local\\UNHUB_Agent";
const QUIT_EVENT: &str = "Local\\UNHUB_Agent_Quit";
const LEGACY_MUTEX: &str = "Local\\LGLITE_Agent";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "UNHUB Agent";
const LEGACY_RUN_VALUE: &str = "LGLITE Agent";
const WM_APP_REFRESH: u32 = WM_APP + 1;

fn mutex_exists(name: &str) -> bool {
    let n = wide(name);
    unsafe {
        let h = OpenMutexW(0x0010_0000 /* SYNCHRONIZE */, 0, n.as_ptr());
        if h.is_null() {
            return false;
        }
        CloseHandle(h);
        true
    }
}

pub fn is_running() -> bool {
    mutex_exists(MUTEX_NAME)
}

/// O agente da versão Python (LGLITE) também captura F13..F24.
pub fn legacy_running() -> bool {
    mutex_exists(LEGACY_MUTEX)
}

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "unhub.exe".into())
}

pub fn start_detached() -> Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(exe())
        .arg("--agent")
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()?;
    Ok(())
}

pub fn request_stop() {
    let n = wide(QUIT_EVENT);
    unsafe {
        let h = OpenEventW(EVENT_MODIFY_STATE, 0, n.as_ptr());
        if !h.is_null() {
            SetEvent(h);
            CloseHandle(h);
        }
    }
}

fn run_value_exists(name: &str) -> bool {
    let (k, v) = (wide(RUN_KEY), wide(name));
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(),
            std::ptr::null_mut(), std::ptr::null_mut()) == 0
    }
}

pub fn autostart_enabled() -> bool {
    run_value_exists(RUN_VALUE)
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let k = wide(RUN_KEY);
    unsafe {
        if enabled {
            let cmd = wide(&format!("\"{}\" --agent", exe()));
            let v = wide(RUN_VALUE);
            let r = RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), REG_SZ,
                cmd.as_ptr() as *const _, (cmd.len() * 2) as u32);
            if r != 0 {
                anyhow::bail!(tr!("err.registry", code = r));
            }
            // evita dois agentes no logon (o do LGLITE em Python e este)
            if run_value_exists(LEGACY_RUN_VALUE) {
                let lv = wide(LEGACY_RUN_VALUE);
                RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), lv.as_ptr());
            }
        } else {
            let v = wide(RUN_VALUE);
            RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr());
        }
    }
    Ok(())
}

// --- estado do agente ---------------------------------------------------------------------

#[derive(Default)]
struct State {
    triggers: HashMap<u32, Macro>, // VK sentinela -> macro
    down: HashSet<u32>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static PAUSED: AtomicBool = AtomicBool::new(false);
static PLAYER: OnceLock<Player> = OnceLock::new();

fn player() -> &'static Player {
    PLAYER.get_or_init(|| Player::new(Arc::new(WinOutput)))
}

fn reload() -> usize {
    let cfg = Config::load();
    crate::i18n::set_language(cfg.language.as_deref());
    let mut triggers = HashMap::new();
    for dev in cfg.triggers.values() {
        for (code, t) in dev {
            let (Ok(code), Some(m)) = (code.parse::<u8>(), cfg.macro_by_id(&t.macro_id)) else { continue };
            if keys::SENTINELS.contains(&code) {
                triggers.insert(keys::sentinel_vk(code), m.clone());
            }
        }
    }
    let n = triggers.len();
    let mut st = STATE.lock().unwrap();
    let down = st.take().map(|s| s.down).unwrap_or_default();
    *st = Some(State { triggers, down });
    n
}

fn trigger_count() -> usize {
    STATE.lock().unwrap().as_ref().map(|s| s.triggers.len()).unwrap_or(0)
}

/// Chamado pelo hook; precisa ser rápido. true = engolir a tecla.
fn on_key(ev: KeyEvent) -> bool {
    if keys::vk_sentinel(ev.vk).is_none() {
        return false;
    }
    let mut guard = STATE.lock().unwrap();
    let Some(st) = guard.as_mut() else { return false };
    let Some(m) = st.triggers.get(&ev.vk) else { return false };
    if PAUSED.load(Ordering::Relaxed) {
        return true; // pausado: engole mesmo assim, para não vazar F13+
    }
    if ev.down {
        if !st.down.insert(ev.vk) {
            return true; // auto-repetição do teclado
        }
        player().press(ev.vk, m);
    } else {
        st.down.remove(&ev.vk);
        player().release(ev.vk);
    }
    true
}

fn config_mtime() -> Option<SystemTime> {
    std::fs::metadata(config::path()).and_then(|m| m.modified()).ok()
}

fn tooltip() -> String {
    let n = trigger_count();
    tr_n!(if PAUSED.load(Ordering::Relaxed) { "tray.paused" } else { "tray.active" }, n)
}

pub fn run() -> Result<()> {
    unsafe {
        let mname = wide(MUTEX_NAME);
        let mutex = CreateMutexW(std::ptr::null(), 0, mname.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        let qname = wide(QUIT_EVENT);
        let quit_event = CreateEventW(std::ptr::null(), 1, 0, qname.as_ptr());
        let main_tid = GetCurrentThreadId();

        reload();
        let hook = KeyboardHook::start(on_key)?;

        // pedido de parada vindo da interface
        let qe = quit_event as usize;
        std::thread::spawn(move || {
            WaitForSingleObject(qe as _, INFINITE);
            PostThreadMessageW(main_tid, WM_QUIT, 0, 0);
        });
        // recarrega quando a interface salva config.json
        std::thread::spawn(move || {
            let mut last = config_mtime();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = config_mtime();
                if now != last {
                    last = now;
                    reload();
                    PostThreadMessageW(main_tid, WM_APP_REFRESH, 0, 0);
                }
            }
        });

        let title = MenuItem::new(tooltip(), false, None);
        let open = MenuItem::new(tr!("tray.open"), true, None);
        let pause = CheckMenuItem::new(tr!("tray.pause"), true, false, None);
        let reload_item = MenuItem::new(tr!("tray.reload"), true, None);
        let quit = MenuItem::new(tr!("tray.quit"), true, None);
        let menu = Menu::new();
        menu.append_items(&[&title, &PredefinedMenuItem::separator(), &open, &pause, &reload_item,
            &PredefinedMenuItem::separator(), &quit])?;
        let icon = Icon::from_rgba(crate::icon::ICON_32.to_vec(), 32, 32)?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip())
            .with_icon(icon)
            .build()?;

        // também reaplica os textos, caso o idioma tenha mudado na interface
        let refresh = |tray: &tray_icon::TrayIcon| {
            let t = tooltip();
            title.set_text(&t);
            let _ = tray.set_tooltip(Some(t));
            open.set_text(tr!("tray.open"));
            pause.set_text(tr!("tray.pause"));
            reload_item.set_text(tr!("tray.reload"));
            quit.set_text(tr!("tray.quit"));
        };
        let open_app = || {
            let _ = std::process::Command::new(exe()).spawn();
        };

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.hwnd.is_null() && msg.message == WM_APP_REFRESH {
                refresh(&tray);
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == open.id() {
                    open_app();
                } else if ev.id == pause.id() {
                    PAUSED.store(pause.is_checked(), Ordering::Relaxed);
                    if pause.is_checked() {
                        player().stop_all();
                    }
                    refresh(&tray);
                } else if ev.id == reload_item.id() {
                    reload();
                    refresh(&tray);
                } else if ev.id == quit.id() {
                    PostQuitMessage(0);
                }
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                    open_app();
                }
            }
        }

        player().stop_all();
        drop(hook);
        drop(tray);
        CloseHandle(quit_event);
        CloseHandle(mutex);
    }
    Ok(())
}
