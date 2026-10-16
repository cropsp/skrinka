//! The Skrinka icon, drawn in code: the tray icon, and `examples/gen_icon.rs` writes it to
//! `assets/skrinka.ico` for the exe.

const BODY: [f32; 3] = [74.0, 108.0, 150.0];
const FRAME: [f32; 3] = [210.0, 224.0, 240.0];

/// RGBA pixels of a `size`×`size` icon: a rounded grey-blue square with a lighter square in the
/// top-left corner. The geometry is laid out on a 32 px grid and scaled.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let size = size as usize;
    let s = size as f32 / 32.0;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let body = rounded_rect_coverage(px, py, [1.0 * s, 1.0 * s, 31.0 * s, 31.0 * s], 6.0 * s);
            if body <= 0.0 {
                continue;
            }
            let frame = rounded_rect_coverage(px, py, [5.0 * s, 5.0 * s, 17.0 * s, 17.0 * s], 2.0 * s);
            let i = (y * size + x) * 4;
            for c in 0..3 {
                rgba[i + c] = (BODY[c] + (FRAME[c] - BODY[c]) * frame).round() as u8;
            }
            rgba[i + 3] = (body * 255.0).round() as u8;
        }
    }
    rgba
}

/// Anti-aliased coverage (0..=1) of the point by a rounded rectangle `[x0, y0, x1, y1]`.
fn rounded_rect_coverage(px: f32, py: f32, [x0, y0, x1, y1]: [f32; 4], radius: f32) -> f32 {
    let dx = (px - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - radius);
    let dy = (py - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - radius);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    let distance = outside + inside - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn every_size_has_transparent_corners_and_an_opaque_body() {
        for size in [16usize, 32, 48, 256] {
            let rgba = icon_rgba(size as u32);
            assert_eq!(rgba.len(), size * size * 4);
            assert_eq!(pixel(&rgba, size, 0, 0)[3], 0, "corner at {size}");
            let body = pixel(&rgba, size, size * 3 / 4, size * 3 / 4);
            assert_eq!(body, [74, 108, 150, 255], "body at {size}");
            let frame = pixel(&rgba, size, size * 11 / 32, size * 11 / 32);
            assert_eq!(frame, [210, 224, 240, 255], "frame at {size}");
        }
    }
}
