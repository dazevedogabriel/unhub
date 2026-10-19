//! UNHUB — configurador leve para mouses Logitech G com fio, sem o G HUB.
//!
//!   unhub.exe            interface
//!   unhub.exe --agent    agente de macros (bandeja)
//!   unhub.exe --status   imprime o estado do mouse (somente leitura)

#![cfg_attr(not(test), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod agent;
mod capture;
mod config;
mod device;
mod hidpp;
mod icon;
mod keys;
mod macros;
mod mouse;
mod onboard;
mod player;
mod ui;
mod util;
mod winput;

use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    i18n::set_language(config::Config::load().language.as_deref());
    if args.iter().any(|a| a == "--agent") {
        if let Err(e) = agent::run() {
            message_box(&tr!("err.agent_crashed", error = e));
        }
        return;
    }
    if args.iter().any(|a| a == "--status") {
        std::process::exit(print_status());
    }
    run_gui();
}

/// Só uma janela: uma segunda execução traz a primeira para a frente.
fn run_gui() {
    let name = util::wide("Local\\UNHUB_App");
    unsafe {
        CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let title = util::wide(util::APP_NAME);
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
            return;
        }
    }
    let icon = eframe::egui::IconData { rgba: icon::ICON_64.to_vec(), width: 64, height: 64 };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(util::APP_NAME)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1120.0, 720.0])
            .with_decorations(false)
            .with_icon(icon),
        // sempre abre no centro do monitor (a posição anterior não é guardada)
        centered: true,
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(util::APP_NAME, options, Box::new(|cc| Ok(Box::new(ui::App::new(cc))))) {
        message_box(&tr!("err.window", error = e));
    }
}

fn message_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let (t, c) = (util::wide(text), util::wide(util::APP_NAME));
    unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn print_status() -> i32 {
    let run = || -> anyhow::Result<()> {
        let api = hidapi::HidApi::new()?;
        let mut m = mouse::Mouse::open_first(&api)?;
        let st = m.status()?;
        let (lo, hi, step) = m.dpi_range();
        let p = m.read_profile()?;
        let line = |label: String, value: String| println!("{:<10}{value}", format!("{label}:"));
        line(tr!("cli.model"), format!("{} ({:04x})", m.name, m.pid));
        line(tr!("cli.mode"), tr!(if st.onboard { "device.onboard" } else { "device.host" }));
        line(tr!("cli.sector"), format!("0x{:04x}", m.sector));
        line("dpi".into(), tr!("cli.dpi", current = st.dpi, lo = lo, hi = hi, step = step));
        line(tr!("cli.levels"), tr!("cli.levels_value", levels = format!("{:?}", p.dpi_levels()),
            default = p.default_dpi_index(), shift = p.shift_dpi_index()));
        line(tr!("cli.rate"), format!("{} Hz", p.report_rate_hz()));
        let cfg = config::Config::load();
        for (i, b) in util::button_names(m.pid, m.memory.button_count).iter().enumerate() {
            for g in [false, true] {
                let desc = match cfg.macro_on_button(m.pid, &p, i, g) {
                    Some(mac) => format!("{} ({})", tr!("buttons.macro", name = mac.name), mac.summary()),
                    None => p.button(i, g).describe(),
                };
                let code = if g { tr!("buttons.gshift_suffix", button = b.code) } else { b.code.clone() };
                println!("{code}: {desc}");
            }
        }
        for (z, name) in util::zones(m.pid, onboard::LED_ZONES) {
            println!("{} {name} ({z}): {:?}", tr!("cli.light"), p.led(z));
        }
        line(tr!("cli.agent"), tr!(if agent::is_running() { "agent.running" } else { "agent.stopped" }));
        Ok(())
    };
    match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", tr!("cli.error", error = e));
            1
        }
    }
}
