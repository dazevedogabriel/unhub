//! Perfis onboard (HID++ feature 0x8100) — formato validado no G403 HERO.
//!
//! Layout do setor do perfil (256 bytes; os dois últimos são CRC-CCITT big-endian):
//!   0      taxa de relatório (1000 / Hz)
//!   1      índice do DPI padrão
//!   2      índice do DPI shift (botão "sniper")
//!   3..12  5 níveis de DPI, uint16 little-endian (0 = nível desligado)
//!   32..95   16 botões, 4 bytes cada
//!   96..159  16 botões na camada G-Shift
//!   208..251 iluminação (ver LedEffect)
//! Demais bytes (energia, nome) são preservados como estão.

use anyhow::{anyhow, bail, Result};

use crate::hidpp::{Device, FEATURE_ONBOARD_PROFILES};
use crate::keys;

pub const MODE_ONBOARD: u8 = 1;
pub const DPI_SLOTS: usize = 5;
pub const BUTTON_SLOTS: usize = 16;
const OFF_RATE: usize = 0;
const OFF_DEFAULT_DPI: usize = 1;
const OFF_SHIFT_DPI: usize = 2;
const OFF_DPI: usize = 3;
const OFF_BUTTONS: usize = 32;
const OFF_GSHIFT: usize = 96;

/// Tipo 0x90: funções especiais executadas pelo firmware.
/// 0x05 (ciclo de DPI) foi observado no G6 do G403; os demais seguem a
/// numeração pública do protocolo (libratbag).
pub const SPECIAL: [(u8, &str); 11] = [
    (0x01, "Rolagem para a esquerda"), (0x02, "Rolagem para a direita"),
    (0x03, "DPI +"), (0x04, "DPI −"), (0x05, "Ciclo de DPI"), (0x06, "DPI padrão"),
    (0x07, "DPI shift (segurar)"), (0x08, "Próximo perfil"), (0x09, "Perfil anterior"),
    (0x0A, "Ciclo de perfis"), (0x0B, "G-Shift (segurar)"),
];

pub fn special_name(code: u8) -> String {
    SPECIAL.iter().find(|s| s.0 == code).map(|s| s.1.to_string()).unwrap_or(format!("Especial 0x{code:02x}"))
}

pub fn mouse_button_name(n: u8) -> String {
    match n {
        1 => "Clique esquerdo".into(),
        2 => "Clique direito".into(),
        3 => "Clique do meio".into(),
        4 => "Voltar".into(),
        5 => "Avançar".into(),
        n => format!("Botão do mouse {n}"),
    }
}

pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// Uma ação de botão, no formato de 4 bytes do perfil onboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Disabled,
    Mouse(u8),
    Key { key: u8, mods: u8 },
    Media(u16),
    Special(u8),
    /// p.ex. macro onboard: preservado intacto
    Raw([u8; 4]),
}

impl Binding {
    pub fn decode(b: &[u8]) -> Self {
        let b: [u8; 4] = b[..4].try_into().unwrap();
        if b == [0xFF; 4] {
            return Binding::Disabled;
        }
        if b[0] == 0x80 && b[1] == 0x01 {
            let mask = u16::from_be_bytes([b[2], b[3]]);
            if mask != 0 && mask & (mask - 1) == 0 {
                return Binding::Mouse(16 - mask.leading_zeros() as u8);
            }
        }
        if b[0] == 0x80 && b[1] == 0x02 {
            return Binding::Key { key: b[3], mods: b[2] };
        }
        if b[0] == 0x80 && b[1] == 0x03 {
            return Binding::Media(u16::from_be_bytes([b[2], b[3]]));
        }
        if b[0] == 0x90 {
            return Binding::Special(b[1]);
        }
        Binding::Raw(b)
    }

    pub fn encode(&self) -> [u8; 4] {
        match *self {
            Binding::Disabled => [0xFF; 4],
            Binding::Mouse(n) => {
                let m = (1u16 << (n.clamp(1, 16) - 1)).to_be_bytes();
                [0x80, 0x01, m[0], m[1]]
            }
            Binding::Key { key, mods } => [0x80, 0x02, mods, key],
            Binding::Media(u) => {
                let m = u.to_be_bytes();
                [0x80, 0x03, m[0], m[1]]
            }
            Binding::Special(s) => [0x90, s, 0xFF, 0xFF],
            Binding::Raw(r) => r,
        }
    }

    pub fn describe(&self) -> String {
        match *self {
            Binding::Disabled => "Desativado".into(),
            Binding::Mouse(n) => mouse_button_name(n),
            Binding::Key { key, mods } => keys::describe_keystroke(mods, key),
            Binding::Media(u) => keys::consumer_name(u),
            Binding::Special(s) => special_name(s),
            Binding::Raw(r) => format!("Outro ({:02x} {:02x} {:02x} {:02x}) — mantido", r[0], r[1], r[2], r[3]),
        }
    }
}

// Iluminação: 11 bytes por zona em 208/219, repetidos em 230/241 (o G HUB grava os dois iguais).
// IDs de efeito = os anunciados pela feature 0x8070 do G403: 0x00, 0x01, 0x03, 0x0A.
const OFF_LEDS: usize = 208;
const OFF_ALT_LEDS: usize = 230;
const LED_SIZE: usize = 11;
pub const LED_ZONES: usize = 2;
pub const FX_OFF: u8 = 0x00;
pub const FX_FIXED: u8 = 0x01;
pub const FX_CYCLE: u8 = 0x03;
pub const FX_BREATH: u8 = 0x0A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedEffect {
    pub effect: u8,
    pub color: [u8; 3],
    pub period_ms: u16,
    pub brightness: u8,
    pub raw: Option<[u8; LED_SIZE]>,
}

impl LedEffect {
    pub fn new(effect: u8) -> Self {
        Self { effect, color: [255, 255, 255], period_ms: 8000, brightness: 100, raw: None }
    }

    pub fn decode(b: &[u8]) -> Self {
        let nz16 = |v: u16, d: u16| if v == 0 { d } else { v };
        let nz8 = |v: u8, d: u8| if v == 0 { d } else { v };
        if b[0] == 0x00 || b[..LED_SIZE].iter().all(|&x| x == 0xFF) {
            return Self::new(FX_OFF);
        }
        match b[0] {
            FX_FIXED => Self { color: [b[1], b[2], b[3]], ..Self::new(FX_FIXED) },
            FX_CYCLE => Self {
                period_ms: nz16(u16::from_be_bytes([b[6], b[7]]), 8000),
                brightness: nz8(b[8], 100),
                ..Self::new(FX_CYCLE)
            },
            FX_BREATH => Self {
                color: [b[1], b[2], b[3]],
                period_ms: nz16(u16::from_be_bytes([b[4], b[5]]), 4000),
                brightness: nz8(b[7], 100),
                ..Self::new(FX_BREATH)
            },
            e => Self { raw: Some(b[..LED_SIZE].try_into().unwrap()), ..Self::new(e) },
        }
    }

    pub fn encode(&self) -> [u8; LED_SIZE] {
        let mut out = [0u8; LED_SIZE];
        out[0] = self.effect;
        match self.effect {
            FX_OFF => {}
            FX_FIXED => out[1..4].copy_from_slice(&self.color),
            FX_CYCLE => {
                out[6..8].copy_from_slice(&self.period_ms.to_be_bytes());
                out[8] = self.brightness;
            }
            FX_BREATH => {
                out[1..4].copy_from_slice(&self.color);
                out[4..6].copy_from_slice(&self.period_ms.to_be_bytes());
                out[7] = self.brightness;
            }
            _ => return self.raw.unwrap_or(out),
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub data: Vec<u8>,
}

impl Profile {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn crc_ok(&self) -> bool {
        let n = self.data.len();
        crc_ccitt(&self.data[..n - 2]) == u16::from_be_bytes([self.data[n - 2], self.data[n - 1]])
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.data.clone();
        let n = out.len();
        let crc = crc_ccitt(&out[..n - 2]).to_be_bytes();
        out[n - 2..].copy_from_slice(&crc);
        out
    }

    // --- DPI -------------------------------------------------------------------
    pub fn report_rate_hz(&self) -> u32 {
        1000 / self.data[OFF_RATE].max(1) as u32
    }

    pub fn set_report_rate_hz(&mut self, hz: u32) -> Result<()> {
        if ![125, 250, 500, 1000].contains(&hz) {
            bail!("taxa deve ser 125, 250, 500 ou 1000 Hz");
        }
        self.data[OFF_RATE] = (1000 / hz) as u8;
        Ok(())
    }

    /// 5 valores; 0 significa nível desligado.
    pub fn dpi_levels(&self) -> [u32; DPI_SLOTS] {
        std::array::from_fn(|i| u16::from_le_bytes([self.data[OFF_DPI + 2 * i], self.data[OFF_DPI + 2 * i + 1]]) as u32)
    }

    pub fn set_dpi_levels(&mut self, levels: &[u32]) -> Result<()> {
        let active: Vec<u32> = levels.iter().copied().filter(|&v| v != 0).collect();
        if active.is_empty() {
            bail!("pelo menos um nível de DPI é obrigatório");
        }
        for i in 0..DPI_SLOTS {
            let v = active.get(i).copied().unwrap_or(0) as u16;
            self.data[OFF_DPI + 2 * i..OFF_DPI + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        let n = active.len().min(DPI_SLOTS) as u8;
        self.data[OFF_DEFAULT_DPI] = self.data[OFF_DEFAULT_DPI].min(n - 1);
        self.data[OFF_SHIFT_DPI] = self.data[OFF_SHIFT_DPI].min(n - 1);
        Ok(())
    }

    fn active_dpi_count(&self) -> usize {
        self.dpi_levels().iter().filter(|&&v| v != 0).count()
    }

    pub fn default_dpi_index(&self) -> usize {
        self.data[OFF_DEFAULT_DPI] as usize
    }

    pub fn set_default_dpi_index(&mut self, i: usize) -> Result<()> {
        if i >= self.active_dpi_count() {
            bail!("índice de DPI fora da tabela");
        }
        self.data[OFF_DEFAULT_DPI] = i as u8;
        Ok(())
    }

    pub fn shift_dpi_index(&self) -> usize {
        self.data[OFF_SHIFT_DPI] as usize
    }

    pub fn set_shift_dpi_index(&mut self, i: usize) -> Result<()> {
        if i >= self.active_dpi_count() {
            bail!("índice de DPI fora da tabela");
        }
        self.data[OFF_SHIFT_DPI] = i as u8;
        Ok(())
    }

    // --- Botões ----------------------------------------------------------------
    pub fn button(&self, index: usize, gshift: bool) -> Binding {
        let o = if gshift { OFF_GSHIFT } else { OFF_BUTTONS } + 4 * index;
        Binding::decode(&self.data[o..o + 4])
    }

    pub fn set_button(&mut self, index: usize, binding: Binding, gshift: bool) {
        assert!(index < BUTTON_SLOTS, "botão fora da faixa");
        let o = if gshift { OFF_GSHIFT } else { OFF_BUTTONS } + 4 * index;
        self.data[o..o + 4].copy_from_slice(&binding.encode());
    }

    // --- Iluminação --------------------------------------------------------------
    pub fn led(&self, zone: usize) -> LedEffect {
        let o = OFF_LEDS + LED_SIZE * zone;
        LedEffect::decode(&self.data[o..o + LED_SIZE])
    }

    pub fn set_led(&mut self, zone: usize, effect: &LedEffect) -> Result<()> {
        if zone >= LED_ZONES {
            bail!("zona inválida");
        }
        if effect.effect == FX_CYCLE || effect.effect == FX_BREATH {
            if !(1000..=20000).contains(&effect.period_ms) {
                bail!("período deve ficar entre 1 e 20 s");
            }
            if !(1..=100).contains(&effect.brightness) {
                bail!("brilho deve ficar entre 1 e 100%");
            }
        }
        let raw = effect.encode();
        for base in [OFF_LEDS, OFF_ALT_LEDS] {
            let o = base + LED_SIZE * zone;
            self.data[o..o + LED_SIZE].copy_from_slice(&raw);
        }
        Ok(())
    }
}

/// Leitura/escrita da memória de perfis via feature 0x8100.
pub struct OnboardMemory {
    pub dev: Device,
    fi: u8,
    pub button_count: usize,
    pub sector_size: usize,
}

impl OnboardMemory {
    pub fn new(mut dev: Device) -> Result<Self> {
        let fi = dev
            .feature_index(FEATURE_ONBOARD_PROFILES)?
            .ok_or_else(|| anyhow!("este dispositivo não tem perfis onboard (0x8100)"))?;
        let info = dev.request(fi, 0, &[])?;
        let sector_size = u16::from_be_bytes([info[7], info[8]]) as usize;
        if !(32..=4096).contains(&sector_size) {
            bail!("tamanho de setor inesperado: {sector_size}");
        }
        Ok(Self { dev, fi, button_count: info[5] as usize, sector_size })
    }

    fn call(&self, func: u8, params: &[u8]) -> Result<[u8; 16]> {
        self.dev.request(self.fi, func, params)
    }

    pub fn mode(&self) -> Result<u8> {
        Ok(self.call(2, &[])?[0])
    }

    pub fn set_mode(&self, mode: u8) -> Result<()> {
        self.call(1, &[mode]).map(|_| ())
    }

    pub fn set_active_profile(&self, sector: u16) -> Result<()> {
        self.call(3, &sector.to_be_bytes()).map(|_| ())
    }

    pub fn set_active_dpi_index(&self, index: u8) -> Result<()> {
        self.call(0x0C, &[index]).map(|_| ())
    }

    pub fn read_sector(&self, sector: u16) -> Result<Vec<u8>> {
        let size = self.sector_size;
        let mut out = vec![0u8; size];
        let mut off = 0;
        while off < size {
            let o = off.min(size - 16); // a leitura de 16 bytes não pode passar do fim
            let s = sector.to_be_bytes();
            let r = self.call(5, &[s[0], s[1], (o >> 8) as u8, o as u8])?;
            out[o..o + 16].copy_from_slice(&r);
            off += 16;
        }
        Ok(out)
    }

    pub fn write_sector(&self, sector: u16, data: &[u8]) -> Result<()> {
        if data.len() != self.sector_size {
            bail!("tamanho de setor inválido");
        }
        let n = data.len();
        if crc_ccitt(&data[..n - 2]) != u16::from_be_bytes([data[n - 2], data[n - 1]]) {
            bail!("CRC inválido; recusando gravar");
        }
        let s = sector.to_be_bytes();
        self.call(6, &[s[0], s[1], 0, 0, (n >> 8) as u8, n as u8])?;
        for chunk in data.chunks(16) {
            self.call(7, chunk)?;
        }
        self.call(8, &[])?;
        if self.read_sector(sector)? != data {
            bail!("a verificação após a gravação falhou");
        }
        Ok(())
    }

    /// Entradas (setor, habilitado) do diretório de perfis do usuário (setor 0).
    pub fn directory(&self) -> Result<Vec<(u16, bool)>> {
        let raw = self.read_sector(0x0000)?;
        let mut entries = Vec::new();
        for c in raw[..raw.len() - 2].chunks_exact(4) {
            let sector = u16::from_be_bytes([c[0], c[1]]);
            if sector == 0xFFFF {
                break;
            }
            entries.push((sector, c[2] == 1));
        }
        Ok(entries)
    }

    /// Setor do perfil editado: o primeiro habilitado do diretório.
    pub fn profile_sector(&self) -> Result<u16> {
        self.directory()?
            .into_iter()
            .find(|(_, enabled)| *enabled)
            .map(|(s, _)| s)
            .ok_or_else(|| anyhow!("nenhum perfil onboard habilitado; restaure o mouse pelo G HUB uma vez"))
    }

    pub fn load_profile(&self) -> Result<(u16, Profile)> {
        let sector = self.profile_sector()?;
        let profile = Profile::new(self.read_sector(sector)?);
        if !profile.crc_ok() {
            bail!("CRC inválido no setor 0x{sector:04x}");
        }
        Ok((sector, profile))
    }

    pub fn save_profile(&self, sector: u16, profile: &Profile) -> Result<()> {
        self.write_sector(sector, &profile.to_bytes())?;
        // recarrega: modo onboard + reativa o perfil para o firmware reler o setor
        self.set_mode(MODE_ONBOARD)?;
        self.set_active_profile(sector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &[u8] = include_bytes!("../tests/fixtures/g403hero_sector0001.bin");
    const ROM: &[u8] = include_bytes!("../tests/fixtures/g403hero_sector0101_rom.bin");

    #[test]
    fn crc_matches_device() {
        let n = USER.len();
        assert_eq!(crc_ccitt(&USER[..n - 2]), u16::from_be_bytes([USER[n - 2], USER[n - 1]]));
    }

    #[test]
    fn roundtrip_is_byte_identical() {
        assert_eq!(Profile::new(USER.to_vec()).to_bytes(), USER);
    }

    #[test]
    fn decode_observed_profile() {
        let p = Profile::new(USER.to_vec());
        assert_eq!(p.report_rate_hz(), 1000);
        assert_eq!(p.dpi_levels(), [400, 1000, 1600, 0, 0]);
        assert_eq!((p.default_dpi_index(), p.shift_dpi_index()), (1, 0));
        for i in 0..5 {
            assert_eq!(p.button(i, false), Binding::Mouse(i as u8 + 1));
        }
        assert_eq!(p.button(5, false), Binding::Special(0x05));
        assert_eq!(p.button(0, true), Binding::Disabled);
    }

    #[test]
    fn bindings_reencode_to_device_bytes() {
        let p = Profile::new(USER.to_vec());
        for i in 0..16 {
            for g in [false, true] {
                let o = if g { 96 } else { 32 } + 4 * i;
                assert_eq!(p.button(i, g).encode(), USER[o..o + 4]);
            }
        }
    }

    #[test]
    fn rom_profile_dpi_table() {
        assert_eq!(Profile::new(ROM.to_vec()).dpi_levels(), [400, 800, 1600, 3200, 0]);
    }

    #[test]
    fn edit_only_touches_expected_bytes() {
        let mut p = Profile::new(USER.to_vec());
        p.set_dpi_levels(&[800, 1600]).unwrap();
        p.set_button(3, Binding::Key { key: 0x06, mods: 1 }, false);
        let out = p.to_bytes();
        let n = USER.len();
        for i in 0..n {
            if out[i] != USER[i] {
                assert!((3..13).contains(&i) || (44..48).contains(&i) || i >= n - 2, "byte {i} mudou");
            }
        }
        assert_eq!(&out[44..48], &[0x80, 0x02, 0x01, 0x06]);
        assert!(Profile::new(out).crc_ok());
    }

    #[test]
    fn dpi_indices_clamped_when_table_shrinks() {
        let mut p = Profile::new(USER.to_vec());
        p.set_default_dpi_index(2).unwrap();
        p.set_dpi_levels(&[800]).unwrap();
        assert_eq!((p.default_dpi_index(), p.shift_dpi_index()), (0, 0));
    }

    #[test]
    fn lighting_decode_observed() {
        let p = Profile::new(USER.to_vec());
        assert_eq!(p.led(0), LedEffect::new(FX_OFF));
        assert_eq!(p.led(1), LedEffect { color: [0xE0, 0x1B, 0x24], ..LedEffect::new(FX_FIXED) });
        let rom = Profile::new(ROM.to_vec());
        assert_eq!((rom.led(0).effect, rom.led(0).period_ms), (FX_CYCLE, 8000));
    }

    #[test]
    fn lighting_writes_both_copies() {
        let mut p = Profile::new(USER.to_vec());
        let e = LedEffect { color: [0, 200, 255], period_ms: 3000, brightness: 70, ..LedEffect::new(FX_BREATH) };
        p.set_led(0, &e).unwrap();
        assert_eq!(&p.data[208..219], &p.data[230..241]);
        assert_eq!(p.led(0), e);
        assert!(p.set_led(0, &LedEffect { period_ms: 500, ..e }).is_err());
    }
}
