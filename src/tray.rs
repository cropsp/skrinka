use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const CAPTURE_REGION: &str = "Capture region";
const CAPTURE_FULLSCREEN: &str = "Capture full screen";
const CAPTURE_WINDOW: &str = "Capture window";

pub struct Tray {
    /// Dropping the `TrayIcon` removes the icon, so it lives as long as `App`.
    _icon: TrayIcon,
    pub capture_region: MenuItem,
    pub capture_fullscreen: MenuItem,
    pub capture_window: MenuItem,
    pub open_folder: MenuItem,
    pub open_config: MenuItem,
    pub reload_config: MenuItem,
    pub autostart: CheckMenuItem,
    /// Debug builds only: saves the raw virtual-screen capture.
    pub debug_dump: Option<MenuItem>,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new(autostart_enabled: bool) -> anyhow::Result<Tray> {
        let capture_region = MenuItem::new(CAPTURE_REGION, true, None);
        let capture_fullscreen = MenuItem::new(CAPTURE_FULLSCREEN, true, None);
        let capture_window = MenuItem::new(CAPTURE_WINDOW, true, None);
        let open_folder = MenuItem::new("Open screenshots folder", true, None);
        let open_config = MenuItem::new("Open config file", true, None);
        let reload_config = MenuItem::new("Reload config", true, None);
        let autostart = CheckMenuItem::new("Run on startup", true, autostart_enabled, None);
        let debug_dump =
            cfg!(debug_assertions).then(|| MenuItem::new("Debug: dump capture", true, None));
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &capture_region,
            &capture_fullscreen,
            &capture_window,
            &PredefinedMenuItem::separator(),
            &open_folder,
            &open_config,
            &reload_config,
            &PredefinedMenuItem::separator(),
            &autostart,
            &PredefinedMenuItem::separator(),
        ])?;
        if let Some(item) = &debug_dump {
            menu.append(item)?;
        }
        menu.append(&quit)?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Skrinka")
            .with_icon(Icon::from_rgba(icon_rgba(), ICON_SIZE as u32, ICON_SIZE as u32)?)
            .build()?;

        Ok(Tray {
            _icon: icon,
            capture_region,
            capture_fullscreen,
            capture_window,
            open_folder,
            open_config,
            reload_config,
            autostart,
            debug_dump,
            quit,
        })
    }

    /// Shows the hotkeys next to the capture items. A tab right-aligns the text the way
    /// Windows menus show shortcuts; it is only a label, not a menu accelerator.
    pub fn set_hotkey_labels(&self, region: &str, fullscreen: &str, window: &str) {
        self.capture_region.set_text(format!("{CAPTURE_REGION}\t{region}"));
        self.capture_fullscreen.set_text(format!("{CAPTURE_FULLSCREEN}\t{fullscreen}"));
        self.capture_window.set_text(format!("{CAPTURE_WINDOW}\t{window}"));
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
