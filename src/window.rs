//! The active window for "capture window". The process is per-monitor DPI aware, so every
//! rectangle here is in physical virtual-screen pixels, the same space as the capture and
//! winit's monitors; nothing is scaled.

use core::ffi::c_void;

use log::{info, warn};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetClientRect, GetForegroundWindow, GetTopWindow, GetWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed,
    GWL_EXSTYLE, GW_HWNDNEXT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

use crate::capture::Capture;
use crate::config::WindowCapture;
use crate::util::{self, Rect};

/// The shell's windows that are in front when the desktop or the taskbar has the focus.
const SHELL_CLASSES: [&str; 4] = ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];
/// Upper bound for the walk down the window list, in case it changes while being walked.
const MAX_WINDOWS: usize = 10_000;

pub struct ForegroundWindow {
    /// Virtual-screen physical pixels.
    pub rect: Rect,
    /// For the log only.
    pub title: String,
    pub maximized: bool,
}

/// The window the user is working in, or `None` (with the reason logged) when there is nothing
/// sensible to capture.
///
/// `from_menu`: the tray menu leaves Skrinka's hidden tray window in the foreground, so then
/// the topmost ordinary window of another program is taken instead, normally the one that was
/// active before the menu opened.
pub fn foreground_window(mode: WindowCapture, from_menu: bool) -> Option<ForegroundWindow> {
    let mut hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        warn!("window capture: there is no foreground window");
        return None;
    }
    if from_menu && is_own(hwnd) {
        hwnd = topmost_other_window()?;
        info!("window capture from the tray menu: using the topmost window {:?}", window_title(hwnd));
    }
    let title = window_title(hwnd);
    if unsafe { IsIconic(hwnd) }.as_bool() {
        warn!("window capture: the foreground window {title:?} is minimized");
        return None;
    }
    if is_own(hwnd) {
        warn!("window capture: the foreground window {title:?} is Skrinka's own");
        return None;
    }
    let class = class_name(hwnd);
    if SHELL_CLASSES.contains(&class.as_str()) {
        warn!("window capture: the desktop or the taskbar is in front ({class})");
        return None;
    }
    let Some(rect) = window_rect(hwnd, mode) else {
        warn!("window capture: no usable {mode:?} rectangle for {title:?}");
        return None;
    };
    let maximized = unsafe { IsZoomed(hwnd) }.as_bool();
    Some(ForegroundWindow { rect, title, maximized })
}

fn window_rect(hwnd: HWND, mode: WindowCapture) -> Option<Rect> {
    let mut rect = RECT::default();
    match mode {
        WindowCapture::Visible => {
            // The window as drawn, without the invisible resize borders of Windows 10/11.
            let bounds = unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut rect as *mut RECT as *mut c_void,
                    std::mem::size_of::<RECT>() as u32,
                )
            };
            if let Err(e) = bounds {
                warn!("window capture: no DWM frame bounds ({e}); using the window rectangle");
                unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
            }
        }
        WindowCapture::Client => {
            // (0, 0, w, h); the origin comes from ClientToScreen.
            unsafe { GetClientRect(hwnd, &mut rect) }.ok()?;
            let mut origin = POINT::default();
            if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
                return None;
            }
            rect.left += origin.x;
            rect.right += origin.x;
            rect.top += origin.y;
            rect.bottom += origin.y;
        }
    }
    rect_from_win32(rect)
}

/// The first window from the top of the Z order that the user could have been working in:
/// visible, not minimized, not cloaked, activatable, not Skrinka's and not part of the shell.
fn topmost_other_window() -> Option<HWND> {
    let skip_styles = (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT).0;
    let mut next = unsafe { GetTopWindow(None) }.ok();
    for _ in 0..MAX_WINDOWS {
        let Some(hwnd) = next else { break };
        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        let candidate = unsafe { IsWindowVisible(hwnd) }.as_bool()
            && !unsafe { IsIconic(hwnd) }.as_bool()
            && ex_style & skip_styles == 0
            && !is_own(hwnd)
            && !is_cloaked(hwnd)
            && !SHELL_CLASSES.contains(&class_name(hwnd).as_str())
            && window_rect(hwnd, WindowCapture::Visible).is_some();
        if candidate {
            return Some(hwnd);
        }
        next = unsafe { GetWindow(hwnd, GW_HWNDNEXT) }.ok();
    }
    warn!("window capture: no window of another program is open");
    None
}

fn is_own(hwnd: HWND) -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid == unsafe { GetCurrentProcessId() }
}

/// Cloaked windows are not shown although "visible": suspended store apps, windows on other
/// virtual desktops.
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

fn window_title(hwnd: HWND) -> String {
    wide_string(|buf| unsafe { GetWindowTextW(hwnd, buf) })
}

fn class_name(hwnd: HWND) -> String {
    wide_string(|buf| unsafe { GetClassNameW(hwnd, buf) })
}

/// `RECT` is half-open: `right` and `bottom` lie just outside. `None` if it is empty.
fn rect_from_win32(r: RECT) -> Option<Rect> {
    let rect = Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
    (rect.w > 0 && rect.h > 0).then_some(rect)
}

/// The text a `GetWindowTextW`-style call writes into a 256-unit buffer, decoded lossily.
fn wide_string(read: impl FnOnce(&mut [u16]) -> i32) -> String {
    let mut buf = [0u16; 256];
    let len = read(&mut buf).clamp(0, buf.len() as i32) as usize;
    String::from_utf16_lossy(&buf[..len])
}

/// The part of the capture (in capture coordinates) to save for a window at `window`. A
/// maximized window's frame reaches past its monitor by the border width, so it is cut to the
/// monitor under its centre. `None` if nothing of it is on screen.
pub fn target_rect(window: Rect, maximized: bool, monitors: &[Rect], capture: &Capture) -> Option<Rect> {
    let on_screen = if maximized {
        let centre = (window.x + window.w / 2, window.y + window.h / 2);
        let monitor = monitors[util::find_containing(monitors, centre)?];
        window.intersect(&monitor)?
    } else {
        window
    };
    capture.screen_to_capture(on_screen).intersect(&capture.rect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITORS: [Rect; 2] = [Rect { x: 0, y: 0, w: 1920, h: 1080 }, Rect { x: 1920, y: 0, w: 1920, h: 1080 }];

    fn capture(x0: i32, width: u32) -> Capture {
        Capture { x0, y0: 0, width, height: 1080, pixels: Vec::new() }
    }

    #[test]
    fn win32_rect_is_half_open() {
        let r = RECT { left: -8, top: 10, right: 100, bottom: 50 };
        assert_eq!(rect_from_win32(r), Some(Rect::new(-8, 10, 108, 40)));
        assert_eq!(rect_from_win32(RECT { left: 5, top: 5, right: 5, bottom: 50 }), None);
        assert_eq!(rect_from_win32(RECT { left: 5, top: 50, right: 10, bottom: 40 }), None);
    }

    #[test]
    fn maximized_window_is_cut_to_its_monitor() {
        // Maximized on monitor 2, the frame sticking out by 8 px on three sides.
        let frame = Rect::new(1912, -8, 1936, 1048);
        let cap = capture(0, 3840);
        assert_eq!(target_rect(frame, true, &MONITORS, &cap), Some(Rect::new(1920, 0, 1920, 1040)));
        // Not maximized: only the capture limits it, so the strip on monitor 1 stays.
        assert_eq!(target_rect(frame, false, &MONITORS, &cap), Some(Rect::new(1912, 0, 1928, 1040)));
    }

    #[test]
    fn window_rect_becomes_capture_coordinates() {
        // Monitor 1 left of the primary one: the capture starts at x = -1920.
        let monitors = [Rect::new(-1920, 0, 1920, 1080), Rect::new(0, 0, 1920, 1080)];
        let cap = capture(-1920, 3840);
        let window = Rect::new(100, 100, 500, 400);
        assert_eq!(target_rect(window, false, &monitors, &cap), Some(Rect::new(2020, 100, 500, 400)));
        assert_eq!(target_rect(window, true, &monitors, &cap), Some(Rect::new(2020, 100, 500, 400)));
    }

    #[test]
    fn window_outside_the_capture_gives_nothing() {
        let cap = capture(0, 3840);
        assert_eq!(target_rect(Rect::new(5000, 0, 100, 100), false, &MONITORS, &cap), None);
        // Maximized with its centre on no monitor.
        assert_eq!(target_rect(Rect::new(5000, 0, 100, 100), true, &MONITORS, &cap), None);
        // Partly off screen: the visible part is kept.
        assert_eq!(target_rect(Rect::new(3800, 1000, 100, 100), false, &MONITORS, &cap), Some(Rect::new(3800, 1000, 40, 80)));
    }
}
