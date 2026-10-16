//! Labels for the overlay: white text on a dark box, rasterized with fontdue into a
//! `0x00RRGGBB` buffer. Without a usable font, drawing does nothing.

use std::path::PathBuf;

use fontdue::{Font, FontSettings};
use log::{debug, warn};

use crate::util::Rect;

const LABEL_PX: f32 = 16.0;
const PADDING: i32 = 4;
const TEXT_COLOR: u32 = 0x00FF_FFFF;
const BOX_COLOR: u32 = 0x001C_1C1C; // (28, 28, 28)

pub struct Text {
    font: Option<Font>,
}

impl Text {
    /// Takes ~60 ms and ~20 MB (fontdue parses every glyph), so it is loaded per overlay and
    /// dropped with it rather than kept for the whole run.
    pub fn load() -> Text {
        let fonts = std::env::var_os("WINDIR")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join("Fonts");
        for name in ["segoeui.ttf", "arial.ttf"] {
            let path = fonts.join(name);
            let font = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| {
                Font::from_bytes(bytes, FontSettings::default()).map_err(str::to_owned)
            });
            match font {
                Ok(font) => {
                    debug!("overlay font: {}", path.display());
                    return Text { font: Some(font) };
                }
                Err(e) => warn!("cannot load {}: {e}", path.display()),
            }
        }
        warn!("no overlay font found; the overlay shows no text");
        Text::none()
    }

    /// Draws nothing.
    pub fn none() -> Text {
        Text { font: None }
    }

    /// Outer size of a label box; `None` without a font.
    pub fn label_size(&self, text: &str) -> Option<(i32, i32)> {
        let font = self.font.as_ref()?;
        let (w, h) = line_size(font, text, LABEL_PX);
        Some((w + 2 * PADDING, h + 2 * PADDING))
    }

    /// Draws a label box with its top-left corner at `at`, clipped to the `bw`×`bh` buffer.
    pub fn draw_label(&self, buffer: &mut [u32], bw: i32, bh: i32, at: (i32, i32), text: &str) {
        let (Some(font), Some((w, h))) = (&self.font, self.label_size(text)) else { return };
        if let Some(r) = Rect::new(at.0, at.1, w, h).intersect(&Rect::new(0, 0, bw, bh)) {
            for row in r.y..r.y + r.h {
                let start = (row * bw + r.x) as usize;
                buffer[start..start + r.w as usize].fill(BOX_COLOR);
            }
        }
        draw_line(font, buffer, bw, bh, (at.0 + PADDING, at.1 + PADDING), text, LABEL_PX);
    }
}

fn line_size(font: &Font, text: &str, px: f32) -> (i32, i32) {
    let width: f32 = text.chars().map(|c| font.metrics(c, px).advance_width).sum();
    let height = font.horizontal_line_metrics(px).map_or(px, |m| m.ascent - m.descent);
    (width.ceil() as i32, height.ceil() as i32)
}

/// One line of text with the top of its line box at `at`.
fn draw_line(font: &Font, buffer: &mut [u32], bw: i32, bh: i32, at: (i32, i32), text: &str, px: f32) {
    let ascent = font.horizontal_line_metrics(px).map_or(px, |m| m.ascent);
    let baseline = at.1 as f32 + ascent;
    let mut pen = at.0 as f32;
    for c in text.chars() {
        let (m, coverage) = font.rasterize(c, px);
        let gx = (pen + m.xmin as f32).round() as i32;
        let gy = (baseline - m.ymin as f32 - m.height as f32).round() as i32;
        for j in 0..m.height as i32 {
            let y = gy + j;
            if !(0..bh).contains(&y) {
                continue;
            }
            for i in 0..m.width as i32 {
                let x = gx + i;
                let alpha = coverage[(j * m.width as i32 + i) as usize];
                if alpha == 0 || !(0..bw).contains(&x) {
                    continue;
                }
                let dst = &mut buffer[(y * bw + x) as usize];
                *dst = blend(*dst, TEXT_COLOR, alpha);
            }
        }
        pen += m.advance_width;
    }
}

/// `fg` over `bg` with coverage `alpha` (0..=255), per channel.
fn blend(bg: u32, fg: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    [16, 8, 0].iter().fold(0, |out, &shift| {
        let (b, f) = ((bg >> shift) & 0xFF, (fg >> shift) & 0xFF);
        out | (((f * a + b * (255 - a) + 127) / 255) << shift)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_endpoints_and_middle() {
        assert_eq!(blend(0x000000, 0xFFFFFF, 255), 0xFFFFFF);
        assert_eq!(blend(0x123456, 0xFFFFFF, 0), 0x123456);
        assert_eq!(blend(0x000000, 0xFFFFFF, 128), 0x808080);
    }

    #[test]
    fn label_draws_box_and_text_and_clips() {
        let text = Text::load();
        let Some((w, h)) = text.label_size("1920 × 1080") else {
            return; // no system font on this machine
        };
        let (bw, bh) = (200, 40);
        let mut buffer = vec![0u32; (bw * bh) as usize];
        text.draw_label(&mut buffer, bw, bh, (10, 5), "1920 × 1080");
        assert_eq!(buffer[(5 * bw + 10) as usize], BOX_COLOR, "box corner");
        assert_eq!(buffer[(4 * bw + 10) as usize], 0, "above the box");
        let inside = &buffer[(5 * bw) as usize..((5 + h) * bw) as usize];
        assert!(inside.iter().any(|&p| (p >> 16) & 0xFF > 0xC0), "some bright text pixel");
        assert!(w > 2 * PADDING && h > 2 * PADDING);
        // Partly or fully outside the buffer: clipped, no panic.
        text.draw_label(&mut buffer, bw, bh, (-50, -10), "1920 × 1080");
        text.draw_label(&mut buffer, bw, bh, (190, 35), "1920 × 1080");
        text.draw_label(&mut buffer, bw, bh, (5000, 5000), "1920 × 1080");
    }
}
