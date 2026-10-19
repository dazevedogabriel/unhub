//! Transporte HID++ 2.0 para mouses Logitech G com fio.
//!
//! O G403 HERO responde HID++ 4.2 no índice de dispositivo 0xFF, pela coleção
//! vendor 0xFF00/usage 2 (relatório longo 0x11, 20 bytes).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};

pub const LOGITECH_VID: u16 = 0x046D;
const REPORT_LONG: u8 = 0x11;
const DEVICE_INDEX_WIRED: u8 = 0xFF;
const SOFTWARE_ID: u8 = 0x0B; // o G HUB usa outro swId; filtramos as respostas por ele

pub const FEATURE_ADJUSTABLE_DPI: u16 = 0x2201;
pub const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;

#[derive(Debug)]
pub struct HidppError {
    pub code: u8,
    pub feature_index: u8,
    pub function: u8,
}

impl std::fmt::Display for HidppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const NAMES: [&str; 10] = ["?", "UNKNOWN", "INVALID_ARGUMENT", "OUT_OF_RANGE", "HW_ERROR",
            "LOGITECH_INTERNAL", "INVALID_FEATURE_INDEX", "INVALID_FUNCTION_ID", "BUSY", "UNSUPPORTED"];
        let name = NAMES.get(self.code as usize).unwrap_or(&"?");
        write!(f, "HID++ erro {} ({name}) feature_idx=0x{:02x} fn={}", self.code, self.feature_index, self.function)
    }
}

impl std::error::Error for HidppError {}

/// Interfaces HID++ longas (usage page 0xFF00, usage 2) de produtos Logitech.
pub fn list_devices(api: &HidApi) -> Vec<DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == LOGITECH_VID && d.usage_page() == 0xFF00 && d.usage() == 0x0002)
        .cloned()
        .collect()
}

pub struct Device {
    h: HidDevice,
    pub pid: u16,
    pub name: String,
    features: HashMap<u16, Option<u8>>,
}

impl Device {
    pub fn open(api: &HidApi, info: &DeviceInfo) -> Result<Self> {
        let h = api.open_path(info.path())?;
        let pid = info.product_id();
        let name = info.product_string().map(str::to_string).unwrap_or_else(|| format!("Logitech {pid:04x}"));
        Ok(Self { h, pid, name, features: HashMap::new() })
    }

    pub fn request(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        if params.len() > 16 {
            bail!("no máximo 16 bytes de parâmetro");
        }
        let fn_sw = (function << 4) | SOFTWARE_ID;
        let mut msg = [0u8; 20];
        msg[0] = REPORT_LONG;
        msg[1] = DEVICE_INDEX_WIRED;
        msg[2] = feature_index;
        msg[3] = fn_sw;
        msg[4..4 + params.len()].copy_from_slice(params);
        self.h.write(&msg)?;
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut r = [0u8; 20];
        while Instant::now() < deadline {
            let n = self.h.read_timeout(&mut r, 50)?;
            if n < 6 || r[1] != DEVICE_INDEX_WIRED {
                continue;
            }
            if r[2] == feature_index && r[3] == fn_sw {
                let mut out = [0u8; 16];
                out.copy_from_slice(&r[4..20]);
                return Ok(out);
            }
            if r[2] == 0xFF && r[3] == feature_index && r[4] == fn_sw {
                return Err(HidppError { code: r[5], feature_index, function }.into());
            }
        }
        Err(anyhow!("sem resposta (feature_idx=0x{feature_index:02x} fn={function})"))
    }

    /// Root.getFeature (fn 0). None se o dispositivo não suporta.
    pub fn feature_index(&mut self, feature_id: u16) -> Result<Option<u8>> {
        if let Some(v) = self.features.get(&feature_id) {
            return Ok(*v);
        }
        let r = self.request(0x00, 0, &[(feature_id >> 8) as u8, feature_id as u8])?;
        let v = (r[0] != 0).then_some(r[0]);
        self.features.insert(feature_id, v);
        Ok(v)
    }

    fn dpi_feature(&mut self) -> Result<u8> {
        self.feature_index(FEATURE_ADJUSTABLE_DPI)?.ok_or_else(|| anyhow!("sem a feature 0x2201"))
    }

    /// Lê a lista do sensor 0: valores ou faixas (0xE000|passo) até um zero.
    pub fn dpi_range(&mut self) -> Result<(u32, u32, Option<u32>)> {
        let fi = self.dpi_feature()?;
        let r = self.request(fi, 1, &[0])?;
        let raw = &r[1..];
        let mut values = Vec::new();
        let mut step = None;
        for c in raw.chunks_exact(2) {
            let v = u16::from_be_bytes([c[0], c[1]]) as u32;
            if v == 0 {
                break;
            }
            if v >> 13 == 0b111 {
                step = Some(v & 0x1FFF);
            } else {
                values.push(v);
            }
        }
        if values.is_empty() {
            bail!("lista de DPI vazia");
        }
        if step.is_some() && values.len() >= 2 {
            return Ok((values[0], *values.last().unwrap(), step));
        }
        Ok((*values.iter().min().unwrap(), *values.iter().max().unwrap(), None))
    }

    pub fn current_dpi(&mut self) -> Result<u32> {
        let fi = self.dpi_feature()?;
        let r = self.request(fi, 2, &[0])?;
        Ok(u16::from_be_bytes([r[1], r[2]]) as u32)
    }

    /// Muda o DPI imediatamente (volátil; o perfil onboard não é alterado).
    pub fn set_dpi_now(&mut self, dpi: u32) -> Result<()> {
        let fi = self.dpi_feature()?;
        self.request(fi, 3, &[0, (dpi >> 8) as u8, dpi as u8])?;
        Ok(())
    }
}
