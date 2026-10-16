use std::cell::Cell;
use std::mem::size_of;

use log::{info, warn};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::core::GUID;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_INFO, NIIF_NONE, NIIF_NOSOUND, NIM_MODIFY,
    NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
};

use crate::strings::{MENU_CAPTURE_FULLSCREEN, MENU_CAPTURE_REGION, MENU_CAPTURE_WINDOW};
use crate::{icon, strings, version};

/// The `uID` tray-icon registered Skrinka's icon under; it does not expose it. tray-icon 0.26.1
/// numbers from a process-wide counter that starts at 1 (`src/counter.rs`):
/// `TrayIconBuilder::new()` takes 1 for the icon's `TrayIconId` (`TrayIconId::new_unique`), and
/// the Windows icon then takes 2 as its `uID` (`platform_impl/windows/mod.rs`,
/// `internal_id = COUNTER.next()`). Skrinka creates exactly one tray icon. This is an internal
/// detail of a pinned version, so `verify_icon_id` checks it at startup.
const TRAY_ICON_UID: u32 = 2;

pub struct Tray {
    /// Dropping the `TrayIcon` removes the icon, so it lives as long as `App`.
    icon: TrayIcon,
    /// Cleared for the rest of the session once the icon id turns out wrong or a notification
    /// fails.
    notifications_ok: Cell<bool>,
    pub capture_region: MenuItem,
    pub capture_fullscreen: MenuItem,
    pub capture_window: MenuItem,
    pub open_folder: MenuItem,
    pub settings: MenuItem,
    /// Debug builds only: saves the raw virtual-screen capture.
    pub debug_dump: Option<MenuItem>,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new() -> anyhow::Result<Tray> {
        let capture_region = MenuItem::new(MENU_CAPTURE_REGION, true, None);
        let capture_fullscreen = MenuItem::new(MENU_CAPTURE_FULLSCREEN, true, None);
        let capture_window = MenuItem::new(MENU_CAPTURE_WINDOW, true, None);
        let open_folder = MenuItem::new(strings::MENU_OPEN_FOLDER, true, None);
        let settings = MenuItem::new(strings::MENU_SETTINGS, true, None);
        let debug_dump = cfg!(debug_assertions).then(|| MenuItem::new(strings::MENU_DEBUG_DUMP, true, None));
        let quit = MenuItem::new(strings::MENU_QUIT, true, None);

        let menu = Menu::new();
        menu.append_items(&[
            &capture_region,
            &capture_fullscreen,
            &capture_window,
            &PredefinedMenuItem::separator(),
            &open_folder,
            &settings,
            &PredefinedMenuItem::separator(),
        ])?;
        if let Some(item) = &debug_dump {
            menu.append(item)?;
        }
        menu.append(&quit)?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(version::title())
            .with_icon(Icon::from_rgba(icon::icon_rgba(ICON_SIZE), ICON_SIZE, ICON_SIZE)?)
            .build()?;

        let tray = Tray {
            icon,
            notifications_ok: Cell::new(true),
            capture_region,
            capture_fullscreen,
            capture_window,
            open_folder,
            settings,
            debug_dump,
            quit,
        };
        tray.verify_icon_id();
        Ok(tray)
    }

    fn hwnd(&self) -> HWND {
        HWND(self.icon.window_handle())
    }

    /// Checks that the shell knows the icon as (`hwnd`, `TRAY_ICON_UID`): at the position
    /// tray-icon reports for it, or, while it sits in the hidden-icons flyout and has no
    /// position, known at all. If not, notifications stay off for the session.
    fn verify_icon_id(&self) {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.hwnd(),
            uID: TRAY_ICON_UID,
            guidItem: GUID::zeroed(),
        };
        // Err for an unknown icon. A hidden icon gives S_FALSE, which is Ok here, while
        // tray-icon's `rect()` (which wants S_OK) is None.
        let ours = unsafe { Shell_NotifyIconGetRect(&id) };
        match (ours, self.icon.rect()) {
            (Ok(rect), Some(theirs)) if same_rect(rect, &theirs) => info!("tray icon id verified"),
            (Ok(_), None) => info!("tray icon id verified (the icon is hidden, without a position)"),
            (ours, theirs) => {
                warn!(
                    "tray icon id {TRAY_ICON_UID} not verified (shell: {ours:?}, tray-icon: {theirs:?}); \
                     notifications are off"
                );
                self.notifications_ok.set(false);
            }
        }
    }

    /// A balloon from the tray icon, which Windows 10/11 show as a toast. Call only from the
    /// main thread, where the tray icon lives.
    pub fn notify(&self, title: &str, text: &str) {
        if !self.notifications_ok.get() {
            return;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd(),
            uID: TRAY_ICON_UID,
            uFlags: NIF_INFO,
            // Windows shows the app's own tray icon; no sound, as ScreenCloud does by default.
            dwInfoFlags: NIIF_NONE | NIIF_NOSOUND,
            ..Default::default()
        };
        copy_utf16(&mut nid.szInfoTitle, title);
        copy_utf16(&mut nid.szInfo, text);
        if unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool() {
            info!("notification: {title}: {}", text.replace('\n', " | "));
        } else {
            warn!("cannot show a notification; notifications are off for this session");
            self.notifications_ok.set(false);
        }
    }

    /// Shows the hotkeys next to the capture items. A tab right-aligns the text the way
    /// Windows menus show shortcuts; it is only a label, not a menu accelerator.
    pub fn set_hotkey_labels(&self, region: &str, fullscreen: &str, window: &str) {
        self.capture_region.set_text(format!("{MENU_CAPTURE_REGION}\t{region}"));
        self.capture_fullscreen.set_text(format!("{MENU_CAPTURE_FULLSCREEN}\t{fullscreen}"));
        self.capture_window.set_text(format!("{MENU_CAPTURE_WINDOW}\t{window}"));
    }
}

/// tray-icon's rect is already in physical pixels.
fn same_rect(rect: RECT, theirs: &tray_icon::Rect) -> bool {
    let position = (theirs.position.x.round() as i32, theirs.position.y.round() as i32);
    let size = (theirs.size.width as i32, theirs.size.height as i32);
    (rect.left, rect.top) == position && (rect.right - rect.left, rect.bottom - rect.top) == size
}

/// `text` as a NUL-terminated UTF-16 string in `buffer`, cut to fit after a whole character
/// (never inside a surrogate pair).
fn copy_utf16(buffer: &mut [u16], text: &str) {
    let room = buffer.len() - 1;
    let mut len = 0;
    for c in text.chars() {
        let mut units = [0u16; 2];
        let units = c.encode_utf16(&mut units);
        if len + units.len() > room {
            break;
        }
        buffer[len..len + units.len()].copy_from_slice(units);
        len += units.len();
    }
    buffer[len] = 0;
}

const ICON_SIZE: u32 = 32;

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&u| u == 0).unwrap();
        String::from_utf16(&buffer[..end]).unwrap()
    }

    #[test]
    fn utf16_text_fits_with_its_nul() {
        let mut buffer = [0xFFFFu16; 64];
        copy_utf16(&mut buffer, "Знімок 2026-10-16.png");
        assert_eq!(decode(&buffer), "Знімок 2026-10-16.png");
    }

    #[test]
    fn long_text_is_cut_on_a_character() {
        let mut buffer = [0xFFFFu16; 64];
        copy_utf16(&mut buffer, &"я".repeat(70));
        assert_eq!(decode(&buffer), "я".repeat(63));
        assert_eq!(buffer[63], 0);
        // An emoji takes two units; with one unit of room left it is dropped whole.
        copy_utf16(&mut buffer, &format!("{}😀", "a".repeat(62)));
        assert_eq!(decode(&buffer), "a".repeat(62));
        copy_utf16(&mut buffer, &format!("{}😀", "a".repeat(61)));
        assert_eq!(decode(&buffer), format!("{}😀", "a".repeat(61)));
    }

    #[test]
    fn tray_rect_comparison() {
        let theirs = tray_icon::Rect {
            position: tray_icon::dpi::PhysicalPosition::new(1641.0, 1000.0),
            size: tray_icon::dpi::PhysicalSize::new(40, 40),
        };
        assert!(same_rect(RECT { left: 1641, top: 1000, right: 1681, bottom: 1040 }, &theirs));
        assert!(!same_rect(RECT { left: 1669, top: 1040, right: 1693, bottom: 1080 }, &theirs));
    }
}
