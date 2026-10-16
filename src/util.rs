use image::RgbaImage;
use log::warn;

use crate::capture::{self, Capture};

/// Pixel rectangle; `w` and `h` count pixels, so a single pixel is `w = h = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, (px, py): (i32, i32)) -> bool {
        self.x <= px && px < self.x + self.w && self.y <= py && py < self.y + self.h
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, ..*self }
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

impl Capture {
    /// The whole capture, in capture coordinates.
    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }

    /// Converts a rectangle in virtual-screen coordinates (as winit reports monitors) to
    /// capture coordinates.
    pub fn screen_to_capture(&self, rect: Rect) -> Rect {
        rect.offset(-self.x0, -self.y0)
    }
}

/// Index of the first rectangle containing the point.
pub fn find_containing(rects: &[Rect], point: (i32, i32)) -> Option<usize> {
    rects.iter().position(|r| r.contains(point))
}

/// Copies `rect` (capture coordinates) out of the capture as RGBA. A rect reaching outside the
/// capture is clamped to it; `None` if nothing is left.
pub fn crop_rgba(capture: &Capture, rect: Rect) -> Option<RgbaImage> {
    let clamped = rect.intersect(&capture.rect())?;
    if clamped != rect {
        warn!("crop {rect:?} reaches outside the {}x{} capture; clamped to {clamped:?}", capture.width, capture.height);
    }
    let (x, w) = (clamped.x as usize, clamped.w as usize);
    let mut bytes = Vec::with_capacity(w * clamped.h as usize * 4);
    for row in clamped.y..clamped.y + clamped.h {
        let start = row as usize * capture.width as usize + x;
        for &p in &capture.pixels[start..start + w] {
            bytes.extend_from_slice(&capture::to_rgba(p));
        }
    }
    RgbaImage::from_raw(clamped.w as u32, clamped.h as u32, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(x0: i32, y0: i32, width: u32, height: u32) -> Capture {
        let pixels = (0..width * height).collect();
        Capture { x0, y0, width, height, pixels }
    }

    #[test]
    fn crop_math_with_negative_capture_origin() {
        let cap = capture(-1920, 0, 1, 1);
        let rect = Rect::new(10, 20, 30, 40);
        // Monitor left of the primary one, at the capture origin.
        assert_eq!(cap.screen_to_capture(rect.offset(-1920, 0)), Rect::new(10, 20, 30, 40));
        // Primary monitor at (0, 0).
        assert_eq!(cap.screen_to_capture(rect.offset(0, 0)), Rect::new(1930, 20, 30, 40));
    }

    #[test]
    fn contains_is_half_open() {
        let r = Rect::new(-1920, 0, 1920, 1080);
        assert!(r.contains((-1920, 0)));
        assert!(r.contains((-1, 1079)));
        assert!(!r.contains((0, 0)));
        assert!(!r.contains((-1, 1080)));
    }

    #[test]
    fn find_monitor_for_point() {
        let monitors = [Rect::new(0, 0, 1920, 1080), Rect::new(1920, 0, 1920, 1080)];
        assert_eq!(find_containing(&monitors, (100, 100)), Some(0));
        assert_eq!(find_containing(&monitors, (1920, 0)), Some(1));
        assert_eq!(find_containing(&monitors, (4000, 0)), None);
    }

    #[test]
    fn crop_copies_the_right_pixels() {
        // Pixel value = its index, so the expected bytes are easy to predict.
        let cap = capture(0, 0, 4, 3);
        let img = crop_rgba(&cap, Rect::new(1, 1, 2, 2)).unwrap();
        assert_eq!(img.dimensions(), (2, 2));
        assert_eq!(img.get_pixel(0, 0).0, capture::to_rgba(5));
        assert_eq!(img.get_pixel(1, 0).0, capture::to_rgba(6));
        assert_eq!(img.get_pixel(0, 1).0, capture::to_rgba(9));
        assert_eq!(img.get_pixel(1, 1).0, capture::to_rgba(10));
    }

    #[test]
    fn crop_is_clamped_to_the_capture() {
        let cap = capture(0, 0, 4, 3);
        let img = crop_rgba(&cap, Rect::new(2, -5, 10, 10)).unwrap();
        assert_eq!(img.dimensions(), (2, 3));
        assert_eq!(img.get_pixel(0, 0).0, capture::to_rgba(2));
        assert!(crop_rgba(&cap, Rect::new(4, 0, 1, 1)).is_none());
    }
}
