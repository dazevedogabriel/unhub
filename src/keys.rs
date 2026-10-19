//! Tabelas de teclas: códigos HID (Keyboard/Keypad page 0x07), modificadores e mídia.
//!
//! O G HUB guarda atalhos como {"code": <HID usage>, "modifiers": [224, ...]} (settings.db),
//! e o perfil onboard usa o mesmo código HID mais um bitmask de modificadores.

/// bit -> (código HID do modificador, nome)
pub const MODIFIERS: [(u8, &str); 8] = [
    (0xE0, "Ctrl"), (0xE1, "Shift"), (0xE2, "Alt"), (0xE3, "Win"),
    (0xE4, "RCtrl"), (0xE5, "RShift"), (0xE6, "AltGr"), (0xE7, "RWin"),
];

pub fn modifier_bit(hid: u16) -> Option<u8> {
    MODIFIERS.iter().position(|(c, _)| *c as u16 == hid).map(|i| 1u8 << i)
}

pub fn key_name(code: u16) -> String {
    let s = match code {
        0x04..=0x1D => return ((b'A' + (code - 0x04) as u8) as char).to_string(),
        0x1E..=0x26 => return ((b'1' + (code - 0x1E) as u8) as char).to_string(),
        0x27 => "0",
        0x3A..=0x45 => return format!("F{}", code - 0x3A + 1),
        0x68..=0x73 => return format!("F{}", code - 0x68 + 13),
        0x59..=0x61 => return format!("Num{}", code - 0x59 + 1),
        0x62 => "Num0",
        0x28 => "Enter", 0x29 => "Esc", 0x2A => "Backspace", 0x2B => "Tab", 0x2C => return tr!("key.space"),
        0x2D => "-", 0x2E => "=", 0x2F => "[", 0x30 => "]", 0x31 => "\\", 0x33 => ";", 0x34 => "'",
        0x35 => "`", 0x36 => ",", 0x37 => ".", 0x38 => "/", 0x39 => "CapsLock",
        0x46 => "PrintScreen", 0x47 => "ScrollLock", 0x48 => "Pause", 0x49 => "Insert",
        0x4A => "Home", 0x4B => "PageUp", 0x4C => "Delete", 0x4D => "End", 0x4E => "PageDown",
        0x4F => return tr!("key.right"), 0x50 => return tr!("key.left"), 0x51 => return tr!("key.down"), 0x52 => return tr!("key.up"),
        0x53 => "NumLock", 0x54 => "Num/", 0x55 => "Num*", 0x56 => "Num-", 0x57 => "Num+",
        0x58 => "NumEnter", 0x63 => "Num.", 0x64 => "\\|(ABNT)", 0x65 => "Menu", 0x87 => "/?(ABNT)",
        0xE0..=0xE7 => MODIFIERS[(code - 0xE0) as usize].1,
        _ => return format!("HID 0x{code:02x}"),
    };
    s.to_string()
}

pub fn describe_keystroke(modifiers: u8, key: u8) -> String {
    let mut parts: Vec<String> = MODIFIERS
        .iter()
        .enumerate()
        .filter(|(i, _)| modifiers & (1 << i) != 0)
        .map(|(_, (_, n))| n.to_string())
        .collect();
    if key != 0 {
        parts.push(key_name(key as u16));
    }
    if parts.is_empty() { tr!("key.none") } else { parts.join(" + ") }
}

/// Consumer page (0x0C): mídia. (usage, chave do nome, VK do Windows)
pub const CONSUMER: [(u16, &str, u16); 12] = [
    (0x00CD, "media.play_pause", 0xB3), (0x00B5, "media.next", 0xB0), (0x00B6, "media.prev", 0xB1),
    (0x00B7, "media.stop", 0xB2), (0x00E9, "media.vol_up", 0xAF), (0x00EA, "media.vol_down", 0xAE),
    (0x00E2, "media.mute", 0xAD), (0x0192, "media.calculator", 0xB7), (0x0194, "media.explorer", 0xB6),
    (0x0223, "media.browser_home", 0xAC), (0x0224, "media.browser_back", 0xA6), (0x0225, "media.browser_forward", 0xA7),
];

pub fn consumer_name(code: u16) -> String {
    CONSUMER.iter().find(|c| c.0 == code).map(|c| tr!(c.1)).unwrap_or(format!("0x{code:04x}"))
}

pub fn consumer_vk(code: u16) -> Option<u16> {
    CONSUMER.iter().find(|c| c.0 == code).map(|c| c.2)
}

/// Pares (Virtual-Key do Windows, código HID).
fn vk_pairs() -> &'static [(u16, u16)] {
    use std::sync::OnceLock;
    static PAIRS: OnceLock<Vec<(u16, u16)>> = OnceLock::new();
    PAIRS.get_or_init(|| {
        let mut v = Vec::new();
        for i in 0..26 { v.push((0x41 + i, 0x04 + i)); }
        for i in 0..9 { v.push((0x31 + i, 0x1E + i)); }
        v.push((0x30, 0x27));
        for i in 0..12 { v.push((0x70 + i, 0x3A + i)); v.push((0x7C + i, 0x68 + i)); }
        for i in 0..9 { v.push((0x61 + i, 0x59 + i)); }
        v.extend_from_slice(&[
            (0x60, 0x62), (0x6A, 0x55), (0x6B, 0x57), (0x6D, 0x56), (0x6E, 0x63), (0x6F, 0x54),
            (0x0D, 0x28), (0x1B, 0x29), (0x08, 0x2A), (0x09, 0x2B), (0x20, 0x2C), (0x14, 0x39),
            (0x2C, 0x46), (0x91, 0x47), (0x13, 0x48), (0x2D, 0x49), (0x24, 0x4A), (0x21, 0x4B),
            (0x2E, 0x4C), (0x23, 0x4D), (0x22, 0x4E), (0x27, 0x4F), (0x25, 0x50), (0x28, 0x51),
            (0x26, 0x52), (0x90, 0x53), (0x5D, 0x65),
            (0xBD, 0x2D), (0xBB, 0x2E), (0xDB, 0x2F), (0xDD, 0x30), (0xDC, 0x31), (0xBA, 0x33),
            (0xDE, 0x34), (0xC0, 0x35), (0xBC, 0x36), (0xBE, 0x37), (0xBF, 0x38),
            (0xE2, 0x64), (0xC1, 0x87), // teclas extras do ABNT2
            (0xA2, 0xE0), (0xA0, 0xE1), (0xA4, 0xE2), (0x5B, 0xE3),
            (0xA3, 0xE4), (0xA1, 0xE5), (0xA5, 0xE6), (0x5C, 0xE7),
        ]);
        v
    })
}

/// VK (do hook de teclado, que distingue esquerda/direita) -> HID.
pub fn vk_to_hid(vk: u32, extended: bool) -> Option<u16> {
    if vk == 0x0D && extended {
        return Some(0x58); // Enter do teclado numérico
    }
    let vk = match vk {
        0x10 => 0xA0, // Shift genérico
        0x11 => if extended { 0xA3 } else { 0xA2 },
        0x12 => if extended { 0xA5 } else { 0xA4 },
        v => v,
    };
    vk_pairs().iter().find(|(v, _)| *v as u32 == vk).map(|(_, h)| *h)
}

pub fn hid_to_vk(hid: u16) -> Option<u16> {
    if hid == 0x58 {
        return Some(0x0D);
    }
    vk_pairs().iter().find(|(_, h)| *h == hid).map(|(v, _)| *v)
}

/// Sentinelas F13..F24: tecla que o botão com macro envia e o agente engole.
pub const SENTINELS: std::ops::RangeInclusive<u8> = 0x68..=0x73;

pub fn sentinel_vk(hid: u8) -> u32 {
    0x7C + (hid - 0x68) as u32
}

pub fn vk_sentinel(vk: u32) -> Option<u8> {
    (0x7C..=0x87).contains(&vk).then(|| (vk - 0x7C) as u8 + 0x68)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(key_name(18), "O"); // card "Open" do G HUB: code 18 + [224] = Ctrl + O
        assert_eq!(MODIFIERS[0].0, 224);
        assert_eq!(key_name(0x3D), "F4");
        assert_eq!(key_name(0x68), "F13");
        assert_eq!(describe_keystroke(0b11, 0x06), "Ctrl + Shift + C");
    }

    #[test]
    fn vk_roundtrip() {
        for &(vk, hid) in vk_pairs() {
            assert_eq!(hid_to_vk(hid), Some(vk));
        }
        assert_eq!(vk_to_hid(0x0D, true), Some(0x58));
        assert_eq!(sentinel_vk(0x68), 0x7C);
        assert_eq!(vk_sentinel(0x87), Some(0x73));
    }
}
