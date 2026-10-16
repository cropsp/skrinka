//! The Settings window and how the event loop talks to it. The window runs on its own thread
//! (`dialog.rs`) and reports back with `UserEvent::SettingsSaved` or `UserEvent::SettingsClosed`.

mod dialog;
mod form;
pub mod hotkey_text;
mod template;

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::thread;

use log::info;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE, WM_CLOSE};

pub use form::Saved;

use crate::app::{send_event, SharedProxy, UserEvent};
use crate::config::Config;

/// The open dialog's HWND, 0 while there is none. The dialog thread writes it.
pub type DialogSlot = Arc<AtomicIsize>;

/// Opens the window on a new thread, showing `config` and `autostart`.
pub fn open(config: Config, autostart: bool, proxy: SharedProxy, slot: DialogSlot) {
    thread::spawn(move || {
        let saved = dialog::run(config, autostart, slot.clone());
        slot.store(0, Ordering::SeqCst);
        let event = match saved {
            Some(saved) => UserEvent::SettingsSaved(saved),
            None => UserEvent::SettingsClosed,
        };
        send_event(&proxy, event);
    });
}

fn hwnd(slot: &DialogSlot) -> Option<HWND> {
    let hwnd = HWND(slot.load(Ordering::SeqCst) as *mut c_void);
    (!hwnd.is_invalid()).then_some(hwnd)
}

/// For a second "Settings…" click. False while the window is still being created.
pub fn bring_to_front(slot: &DialogSlot) -> bool {
    let Some(hwnd) = hwnd(slot) else { return false };
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

/// Asks the window to close as Cancel does. Posted, never sent: it lives on another thread.
pub fn close(slot: &DialogSlot) {
    if let Some(hwnd) = hwnd(slot) {
        info!("closing the Settings window");
        let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
}
