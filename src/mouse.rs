//! Sessão com um mouse: junta transporte, memória onboard e backup.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use hidapi::HidApi;

use crate::hidpp::{self, Device, HidppError};
use crate::onboard::{OnboardMemory, Profile, MODE_ONBOARD};
use crate::util;

pub struct Mouse {
    pub memory: OnboardMemory,
    pub pid: u16,
    pub name: String,
    pub sector: u16,
    original: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub onboard: bool,
    pub dpi: u32,
}

impl Mouse {
    pub fn open_first(api: &HidApi) -> Result<Self> {
        let mut errors = Vec::new();
        for info in hidpp::list_devices(api) {
            match Device::open(api, &info).and_then(Self::from_device) {
                Ok(m) => return Ok(m),
                // interface sem 0x8100, receptor, etc.
                Err(e) => errors.push(format!("{}: {e}", info.product_string().unwrap_or("?"))),
            }
        }
        let extra = if errors.is_empty() { String::new() } else { format!(" ({})", errors.join("; ")) };
        Err(anyhow!("{}{extra}", tr!("err.no_device")))
    }

    fn from_device(dev: Device) -> Result<Self> {
        let pid = dev.pid;
        let name = util::model_name(pid, &dev.name);
        let memory = OnboardMemory::new(dev)?;
        let (sector, profile) = memory.load_profile()?;
        Ok(Self { memory, pid, name, sector, original: profile.to_bytes() })
    }

    pub fn read_profile(&mut self) -> Result<Profile> {
        let (sector, p) = self.memory.load_profile()?;
        self.sector = sector;
        Ok(p)
    }

    pub fn dpi_range(&mut self) -> (u32, u32, u32) {
        match self.memory.dev.dpi_range() {
            Ok((lo, hi, step)) => (lo, hi, step.unwrap_or(1)),
            Err(_) => (100, 25600, 50),
        }
    }

    pub fn status(&mut self) -> Result<Status> {
        let onboard = self.memory.mode()? == MODE_ONBOARD;
        let dpi = self.memory.dev.current_dpi()?;
        Ok(Status { onboard, dpi })
    }

    pub fn set_dpi_now(&mut self, dpi: u32) -> Result<()> {
        self.memory.dev.set_dpi_now(dpi)
    }

    /// Grava o perfil e ativa o modo onboard. Faz backup do original uma vez.
    pub fn apply(&mut self, profile: &Profile) -> Result<()> {
        if util::original_backup(self.pid, self.sector).is_none() {
            util::backup_sector(self.pid, self.sector, &self.original)?;
        }
        self.memory.save_profile(self.sector, profile)?;
        // o firmware volta ao DPI padrão do perfil; garantimos o índice explicitamente
        match self.memory.set_active_dpi_index(profile.default_dpi_index() as u8) {
            Err(e) if e.downcast_ref::<HidppError>().is_none() => Err(e),
            _ => Ok(()),
        }
    }

    pub fn restore_original(&mut self) -> Result<PathBuf> {
        let path = util::original_backup(self.pid, self.sector)
            .ok_or_else(|| anyhow!(tr!("err.no_backup")))?;
        let data = std::fs::read(&path)?;
        self.memory.save_profile(self.sector, &Profile::new(data))?;
        Ok(path)
    }
}
