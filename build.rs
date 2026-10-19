//! Rasteriza assets/logo.svg: ícone .ico embutido no executável (Explorer) e buffers RGBA
//! usados pela janela, pela bandeja e pela barra lateral.

use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

fn render(tree: &usvg::Tree, size: u32, pad: f32) -> tiny_skia::Pixmap {
    let mut pm = tiny_skia::Pixmap::new(size, size).unwrap();
    let ts = tree.size();
    let inner = size as f32 * (1.0 - 2.0 * pad);
    let s = inner / ts.width().max(ts.height());
    let dx = (size as f32 - ts.width() * s) / 2.0;
    let dy = (size as f32 - ts.height() * s) / 2.0;
    resvg::render(tree, tiny_skia::Transform::from_row(s, 0.0, 0.0, s, dx, dy), &mut pm.as_mut());
    pm
}

fn rgba(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    pm.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// ICO com imagens PNG (aceito pelo Windows desde o Vista).
fn write_ico(path: &Path, pngs: &[(u32, Vec<u8>)]) {
    let mut out = vec![0, 0, 1, 0, pngs.len() as u8, 0];
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, png) in pngs {
        let b = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[b, b, 0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in pngs {
        out.extend_from_slice(png);
    }
    std::fs::write(path, out).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=assets/logo.svg");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let svg = std::fs::read("assets/logo.svg").unwrap();
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("logo.svg inválido");

    // ícones (janela, bandeja): pequena margem; logo da interface: sem margem
    for (name, size, pad) in [("icon_64", 64, 0.06), ("icon_32", 32, 0.04), ("logo_128", 128, 0.0)] {
        std::fs::write(out.join(format!("{name}.rgba")), rgba(&render(&tree, size, pad))).unwrap();
    }

    let pngs: Vec<(u32, Vec<u8>)> = [16u32, 20, 24, 32, 40, 48, 64, 128, 256]
        .iter()
        .map(|&s| (s, render(&tree, s, 0.04).encode_png().unwrap()))
        .collect();
    let ico = out.join("unhub.ico");
    write_ico(&ico, &pngs);

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico.to_str().unwrap());
        res.set("FileDescription", "UNHUB — configurador de mouses Logitech G");
        res.set("ProductName", "UNHUB");
        res.compile().expect("falha ao embutir o ícone (rc.exe)");
    }
}
