//! Writes `assets/skrinka.ico` (16, 32, 48 and 256 px) from the tray icon drawing:
//! `cargo run --example gen_icon`. Run it after changing `src/icon.rs` and commit the result.

#[path = "../src/icon.rs"]
mod icon;

use std::fs::{self, File};
use std::io::BufWriter;

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::{ExtendedColorType, ImageResult};

const SIZES: [u32; 4] = [16, 32, 48, 256];
const PATH: &str = "assets/skrinka.ico";

fn main() -> ImageResult<()> {
    let pixels: Vec<Vec<u8>> = SIZES.iter().map(|&size| icon::icon_rgba(size)).collect();
    let frames = SIZES
        .iter()
        .zip(&pixels)
        .map(|(&size, rgba)| IcoFrame::as_png(rgba, size, size, ExtendedColorType::Rgba8))
        .collect::<ImageResult<Vec<_>>>()?;
    fs::create_dir_all("assets")?;
    IcoEncoder::new(BufWriter::new(File::create(PATH)?)).encode_images(&frames)?;
    println!("wrote {PATH}");
    Ok(())
}
