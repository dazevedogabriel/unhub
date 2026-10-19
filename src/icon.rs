//! Logo (assets/logo.svg), rasterizado em tempo de compilação pelo build.rs.

pub const ICON_64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon_64.rgba"));
pub const ICON_32: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon_32.rgba"));
pub const LOGO_128: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo_128.rgba"));
