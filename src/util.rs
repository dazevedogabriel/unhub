use std::path::{Path, PathBuf};

use anyhow::Result;

pub const APP_NAME: &str = "UNHUB";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// %APPDATA%\UNHUB. Na primeira execução copia config e backups do LGLITE (versão Python),
/// para que o backup mais antigo continue sendo o perfil anterior a qualquer gravação.
pub fn data_dir() -> PathBuf {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let base = appdata.join(APP_NAME);
    if !base.exists() {
        let _ = migrate_from_lglite(&appdata.join("LGLITE"), &base);
    }
    let _ = std::fs::create_dir_all(base.join("backups"));
    base
}

fn migrate_from_lglite(old: &Path, new: &Path) -> std::io::Result<()> {
    if !old.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(new.join("backups"))?;
    if old.join("config.json").exists() {
        std::fs::copy(old.join("config.json"), new.join("config.json"))?;
    }
    if let Ok(rd) = std::fs::read_dir(old.join("backups")) {
        for e in rd.flatten() {
            std::fs::copy(e.path(), new.join("backups").join(e.file_name()))?;
        }
    }
    Ok(())
}

pub fn backup_sector(pid: u16, sector: u16, data: &[u8]) -> Result<PathBuf> {
    let name = format!("{pid:04x}_sector{sector:04x}_{}.bin", timestamp());
    let path = data_dir().join("backups").join(name);
    std::fs::write(&path, data)?;
    Ok(path)
}

/// O mais antigo = estado anterior ao primeiro uso.
pub fn original_backup(pid: u16, sector: u16) -> Option<PathBuf> {
    let prefix = format!("{pid:04x}_sector{sector:04x}_");
    let mut files: Vec<PathBuf> = std::fs::read_dir(data_dir().join("backups"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with(&prefix) && n.ends_with(".bin")
        })
        .collect();
    files.sort();
    files.into_iter().next()
}

fn timestamp() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Nomes de processos em execução (minúsculos).
pub fn running_processes() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push(String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase());
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// O agente do G HUB põe o mouse em modo host e sobrescreve o comportamento onboard.
pub fn ghub_running() -> bool {
    const GHUB: [&str; 3] = ["lghub_agent.exe", "lghub.exe", "lghub_system_tray.exe"];
    running_processes().iter().any(|p| GHUB.contains(&p.as_str()))
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// --- nomes por modelo ------------------------------------------------------------------

pub struct ButtonInfo {
    pub code: String,
    pub name: String,
}

/// G403 HERO: ordem dos slots g403hero_g1..g6 (metadata.json do G HUB); o perfil de
/// fábrica mapeia G1..G5 para os botões 1..5 e G6 para o ciclo de DPI.
pub fn button_names(pid: u16, count: usize) -> Vec<ButtonInfo> {
    let known: &[&str] = match pid {
        0xC08F => &["btn.left", "btn.right", "btn.middle", "btn.rear", "btn.front", "btn.dpi"],
        _ => &[],
    };
    (0..count)
        .map(|i| ButtonInfo {
            code: format!("G{}", i + 1),
            name: known.get(i).map(|k| tr!(*k)).unwrap_or_else(|| tr!("btn.n", n = i + 1)),
        })
        .collect()
}

/// Zonas de LED na ordem de exibição: (índice da zona no perfil, nome).
/// No G403 a zona 0 é o logo e a zona 1 é a faixa primária (conferido no mouse);
/// a interface mostra a Primária primeiro, como no layout.
pub fn zones(pid: u16, count: usize) -> Vec<(usize, String)> {
    match pid {
        0xC08F if count >= 2 => vec![(1, tr!("zone.primary")), (0, tr!("zone.logo"))],
        _ => (0..count).map(|i| (i, tr!("zone.n", n = i + 1))).collect(),
    }
}

pub fn model_name(pid: u16, fallback: &str) -> String {
    match pid {
        0xC08F => "G403 HERO".into(),
        _ => fallback.trim_start_matches("Logitech ").to_string(),
    }
}
