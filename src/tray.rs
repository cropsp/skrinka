use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    /// Dropping the `TrayIcon` removes the icon, so it lives as long as `App`.
    _icon: TrayIcon,
    pub debug_dump: MenuItem,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new() -> anyhow::Result<Tray> {
        let debug_dump = MenuItem::new("Debug: dump capture", true, None);
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append_items(&[&debug_dump, &PredefinedMenuItem::separator(), &quit])?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Skrinka")
            .with_icon(Icon::from_rgba(icon_rgba(), ICON_SIZE as u32, ICON_SIZE as u32)?)
            .build()?;

        Ok(Tray { _icon: icon, debug_dump, quit })
    }
}

const ICON_SIZE: usize = 32;
const BODY: [f32; 3] = [74.0, 108.0, 150.0];
const FRAME: [f32; 3] = [210.0, 224.0, 240.0];

/// A rounded grey-blue square with a lighter 12×12 square in the top-left corner.
fn icon_rgba() -> Vec<u8> {
    let mut rgba = vec![0u8; ICON_SIZE * ICON_SIZE * 4];
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let body = rounded_rect_coverage(px, py, [1.0, 1.0, 31.0, 31.0], 6.0);
            if body <= 0.0 {
                continue;
            }
            let frame = rounded_rect_coverage(px, py, [5.0, 5.0, 17.0, 17.0], 2.0);
            let i = (y * ICON_SIZE + x) * 4;
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
