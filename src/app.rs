use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicIsize;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use log::{error, info, warn};
use tray_icon::menu::{MenuEvent, MenuId};
use tray_icon::TrayIconEvent;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

use crate::autostart;
use crate::capture::{self, Capture};
use crate::config::{Config, FullscreenMode, Notifications};
use crate::dialog;
use crate::hotkeys::Hotkeys;
use crate::output::{self, Action, WorkerResult};
use crate::overlay::{OverlaySession, Outcome};
use crate::settings::{self, DialogSlot};
use crate::strings;
use crate::text::Text;
use crate::tray::Tray;
use crate::util::{self, Rect};
use crate::window;

/// `EventLoopProxy` is `Send` but not `Sync` on Windows, while the tauri-apps event handlers
/// must be `Send + Sync`, hence the mutex.
pub type SharedProxy = Arc<Mutex<EventLoopProxy<UserEvent>>>;

#[derive(Debug)]
pub enum UserEvent {
    HotkeyRegion,
    HotkeyFullscreen,
    HotkeyWindow,
    TrayMenu(MenuId),
    TrayDoubleClick,
    /// A capture chosen from the tray menu, sent once the menu has disappeared.
    DelayedCapture(CaptureKind),
    WorkerDone(WorkerResult),
    /// The Settings window closed with Save (and passed its checks).
    SettingsSaved(settings::Saved),
    /// The Settings window closed with Cancel, Esc or its close button.
    SettingsClosed,
}

#[derive(Debug, Clone, Copy)]
pub enum CaptureKind {
    Region,
    Fullscreen,
    Window,
}

/// Time for the tray menu to vanish before the screen is captured.
const MENU_CLOSE_DELAY: Duration = Duration::from_millis(300);

enum TrayAction {
    Capture(CaptureKind),
    OpenFolder,
    Settings,
    DebugDump,
    Quit,
}

pub fn send_event(proxy: &SharedProxy, event: UserEvent) {
    let proxy = proxy.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Fails only once the event loop is gone, and then nobody is listening anyway.
    let _ = proxy.send_event(event);
}

pub struct App {
    proxy: SharedProxy,
    config: Config,
    config_path: PathBuf,
    initialized: bool,
    tray: Option<Tray>,
    hotkeys: Option<Hotkeys>,
    overlay: Option<OverlaySession>,
    /// A "Save as" dialog is open; a new overlay (always on top) would hide it.
    save_as_open: bool,
    /// The Settings window is open (or being created). The hotkeys are unregistered meanwhile,
    /// so they reach its hotkey fields, and captures from the menu are ignored: the overlay
    /// would cover the window.
    settings_open: bool,
    settings_window: DialogSlot,
    fatal_error: Option<String>,
}

impl App {
    pub fn new(config: Config, config_path: PathBuf, proxy: SharedProxy) -> App {
        App {
            proxy,
            config,
            config_path,
            initialized: false,
            tray: None,
            hotkeys: None,
            overlay: None,
            save_as_open: false,
            settings_open: false,
            settings_window: Arc::new(AtomicIsize::new(0)),
            fatal_error: None,
        }
    }

    /// Set when the app had to stop because it could not start properly.
    pub fn take_fatal_error(&mut self) -> Option<String> {
        self.fatal_error.take()
    }

    fn init(&mut self, event_loop: &ActiveEventLoop) {
        match Tray::new() {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => {
                error!("cannot create the tray icon: {e:#}");
                self.fatal_error = Some(format!("Could not create the tray icon: {e:#}"));
                event_loop.exit();
                return;
            }
        }

        // muda and tray-icon keep only the first handler they are given; this runs once.
        let proxy = self.proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            send_event(&proxy, UserEvent::TrayMenu(e.id));
        }));
        let proxy = self.proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { .. } = e {
                send_event(&proxy, UserEvent::TrayDoubleClick);
            }
        }));

        match Hotkeys::new(self.proxy.clone()) {
            Ok(hotkeys) => {
                self.hotkeys = Some(hotkeys);
                self.register_hotkeys();
            }
            Err(e) => {
                error!("cannot set up global hotkeys: {e:#}");
                dialog::error_async(format!("Could not set up the global hotkeys: {e:#}"));
            }
        }

        log_monitors(event_loop);
        info!("ready; screenshots go to {}", self.config.save_dir.display());
    }

    /// (Re)registers the hotkeys from the current config and shows them in the menu.
    fn register_hotkeys(&mut self) {
        let Some(hotkeys) = &mut self.hotkeys else { return };
        if let Some(problems) = hotkeys.register(&self.config) {
            dialog::warning_async(problems);
        }
        if let Some(tray) = &self.tray {
            tray.set_hotkey_labels(&hotkeys.region_label, &hotkeys.fullscreen_label, &hotkeys.window_label);
        }
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: MenuId) {
        let Some(tray) = &self.tray else { return };
        let action = if id == *tray.capture_region.id() {
            TrayAction::Capture(CaptureKind::Region)
        } else if id == *tray.capture_fullscreen.id() {
            TrayAction::Capture(CaptureKind::Fullscreen)
        } else if id == *tray.capture_window.id() {
            TrayAction::Capture(CaptureKind::Window)
        } else if id == *tray.open_folder.id() {
            TrayAction::OpenFolder
        } else if id == *tray.settings.id() {
            TrayAction::Settings
        } else if tray.debug_dump.as_ref().is_some_and(|item| id == *item.id()) {
            TrayAction::DebugDump
        } else if id == *tray.quit.id() {
            TrayAction::Quit
        } else {
            return;
        };
        match action {
            TrayAction::Capture(kind) => {
                if self.settings_blocks_capture() {
                    return;
                }
                let proxy = self.proxy.clone();
                thread::spawn(move || {
                    thread::sleep(MENU_CLOSE_DELAY);
                    send_event(&proxy, UserEvent::DelayedCapture(kind));
                });
            }
            TrayAction::OpenFolder => self.open_folder(),
            TrayAction::Settings => self.open_settings(),
            TrayAction::DebugDump => self.debug_dump_capture(),
            TrayAction::Quit => {
                info!("quit from the tray menu");
                if self.settings_open {
                    settings::close(&self.settings_window);
                }
                event_loop.exit();
            }
        }
    }

    /// Captures wait until the Settings window is closed.
    fn settings_blocks_capture(&self) -> bool {
        if self.settings_open {
            info!("capture ignored: the Settings window is open");
        }
        self.settings_open
    }

    fn open_folder(&self) {
        let dir = &self.config.save_dir;
        if let Err(e) = fs::create_dir_all(dir) {
            error!("cannot create {}: {e}", dir.display());
            dialog::error_async(format!("Could not create {}: {e}", dir.display()));
            return;
        }
        spawn_program(Command::new("explorer.exe").arg(dir));
    }

    /// Opens the Settings window, or brings it to the front if it is already open.
    fn open_settings(&mut self) {
        if self.settings_open {
            if settings::bring_to_front(&self.settings_window) {
                info!("settings window brought to the front");
            }
            return;
        }
        // From the file, so edits made by hand are shown and applied by Save.
        let config = match Config::load(&self.config_path) {
            Ok(config) => config,
            Err(e) => {
                warn!("Settings shows the settings in use: {e:#}");
                self.config.clone()
            }
        };
        if config != self.config {
            info!("config.toml has changes that are not in use yet; Settings shows them");
        }
        if let Some(hotkeys) = &mut self.hotkeys {
            hotkeys.unregister_all();
        }
        self.settings_open = true;
        info!("opening the Settings window");
        settings::open(config, autostart::is_enabled(), self.proxy.clone(), self.settings_window.clone());
    }

    fn on_settings_saved(&mut self, saved: settings::Saved) {
        self.settings_open = false;
        let mut changed = false;
        let mut failed = false;
        if saved.config != self.config {
            changed = true;
            match saved.config.save(&self.config_path) {
                Ok(()) => info!("settings saved to {}", self.config_path.display()),
                Err(e) => {
                    failed = true;
                    error!("{e:#}");
                    dialog::error_async(format!("{e:#}\n\nThe new settings are in use until Skrinka exits."));
                }
            }
            self.config = saved.config;
        }
        if saved.autostart != autostart::is_enabled() {
            changed = true;
            match autostart::set_enabled(saved.autostart) {
                Ok(()) => info!("run on startup: {}", saved.autostart),
                Err(e) => {
                    failed = true;
                    error!("{e:#}");
                    dialog::error_async(format!("Could not change \"Run on startup\": {e:#}"));
                }
            }
        }
        self.register_hotkeys();
        if !changed {
            info!("settings saved without changes");
        } else if !failed {
            info!("settings applied");
            // Not subject to `notifications`: it confirms an action the user just took.
            if let Some(tray) = &self.tray {
                tray.notify(strings::APP_NAME, strings::NOTIFY_SETTINGS_SAVED);
            }
        }
        release_freed_memory();
    }

    fn on_settings_closed(&mut self) {
        self.settings_open = false;
        info!("settings window closed without saving");
        self.register_hotkeys();
        release_freed_memory();
    }

    fn capture_region(&mut self, event_loop: &ActiveEventLoop) {
        if self.overlay.is_some() || self.settings_blocks_capture() {
            return;
        }
        if self.save_as_open {
            info!("region capture ignored: the Save as dialog is still open");
            return;
        }
        let started = Instant::now();
        // The font is parsed while the screen is captured and the windows are built.
        let text = thread::spawn(Text::load);
        // Captured before any overlay window exists, so none of them is in the shot.
        let Some(capture) = capture_or_report() else { return };
        match OverlaySession::open(event_loop, capture, text) {
            Ok(overlay) => {
                info!("region overlay ready in {} ms", started.elapsed().as_millis());
                self.overlay = Some(overlay);
            }
            Err(e) => {
                error!("cannot open the region overlay: {e:#}");
                dialog::error_async(format!("Could not open the region overlay: {e:#}"));
            }
        }
    }

    fn on_overlay_event(&mut self, id: WindowId, event: WindowEvent) {
        let Some(overlay) = &mut self.overlay else { return };
        match overlay.window_event(id, event) {
            Outcome::Continue => {}
            Outcome::Cancel => {
                info!("region capture cancelled");
                self.overlay = None;
                release_freed_memory();
            }
            Outcome::Finish(image, action) => {
                // The overlay goes first, so a dialog can never open underneath it.
                self.overlay = None;
                self.save_as_open = action == Action::SaveAs;
                output::spawn(image, action, self.config.clone(), self.proxy.clone());
            }
        }
    }

    fn capture_fullscreen(&mut self, event_loop: &ActiveEventLoop) {
        if self.overlay.is_some() || self.settings_blocks_capture() {
            return;
        }
        let Some(capture) = capture_or_report() else { return };
        let rect = match self.config.fullscreen_mode {
            FullscreenMode::AllMonitors => capture.rect(),
            FullscreenMode::CursorMonitor => cursor_monitor_in(event_loop, &capture),
        };
        let Some(image) = util::crop_rgba(&capture, rect) else {
            error!("full-screen capture: {rect:?} is outside the capture");
            return;
        };
        info!("full-screen capture of {rect:?}");
        output::spawn(image, Action::ClipboardAndAutoSave, self.config.clone(), self.proxy.clone());
    }

    /// The active window; the monitor under the cursor when there is no sensible window, so
    /// the hotkey always produces a screenshot. Screen-based: whatever overlaps the window is
    /// in the picture.
    fn capture_window(&mut self, event_loop: &ActiveEventLoop, from_menu: bool) {
        if self.overlay.is_some() || self.settings_blocks_capture() {
            return;
        }
        let Some(capture) = capture_or_report() else { return };
        let monitors = monitor_rects(event_loop);
        let target = window::foreground_window(self.config.window_capture, from_menu).and_then(|win| {
            let rect = window::target_rect(win.rect, win.maximized, &monitors, &capture);
            let maximized = if win.maximized { " (maximized)" } else { "" };
            match rect {
                Some(rect) => info!("window capture of {:?} at {:?}{maximized} → capture {rect:?}", win.title, win.rect),
                None => warn!("window capture: {:?} at {:?}{maximized} is not on screen", win.title, win.rect),
            }
            rect
        });
        let rect = target.unwrap_or_else(|| {
            let rect = cursor_monitor_in(event_loop, &capture);
            warn!("window capture falls back to the monitor under the cursor: capture {rect:?}");
            rect
        });
        let Some(image) = util::crop_rgba(&capture, rect) else {
            error!("window capture: {rect:?} is outside the capture");
            return;
        };
        output::spawn(image, Action::ClipboardAndAutoSave, self.config.clone(), self.proxy.clone());
    }

    fn on_worker_done(&mut self, result: WorkerResult) {
        // Also covers the overlay of a finished region capture, dropped when the worker started.
        release_freed_memory();
        match (&result.error, &result.saved_path) {
            (Some(_), _) => info!("{:?} finished with errors", result.action),
            (None, Some(path)) => info!("{:?} finished: {}", result.action, path.display()),
            (None, None) => info!("{:?} finished", result.action),
        }
        if result.action == Action::SaveAs {
            self.save_as_open = false;
            if let Some(dir) = result.saved_path.as_deref().and_then(Path::parent) {
                self.remember_save_as_dir(dir);
            }
        }
        // Errors have already been shown in a dialog.
        if result.error.is_none() {
            self.notify_done(&result);
        }
    }

    /// The tray notification for a finished screenshot, as `notifications` asks for.
    fn notify_done(&self, result: &WorkerResult) {
        let Some(tray) = &self.tray else { return };
        match (self.config.notifications, &result.saved_path, result.action) {
            (Notifications::Off, _, _) => {}
            (_, Some(path), _) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let folder = path.parent().map(Path::display);
                let folder = folder.map(|f| f.to_string()).unwrap_or_default();
                tray.notify(strings::NOTIFY_SAVED, &format!("{name}\n{folder}"));
            }
            (Notifications::All, None, Action::ClipboardOnly) => {
                let (w, h) = result.size;
                tray.notify(strings::NOTIFY_COPIED, &format!("{w} × {h}"));
            }
            // "Save as" cancelled, or clipboard only with `saves`.
            _ => {}
        }
    }

    /// Stores the folder of the last "Save as" in the config file, only when it changed.
    fn remember_save_as_dir(&mut self, dir: &Path) {
        if dir == self.config.last_save_as_dir {
            return;
        }
        self.config.last_save_as_dir = dir.to_path_buf();
        // Start from the file, not from memory: edits the user has not reloaded yet must
        // survive. If the file is broken right now, it is left alone.
        let on_disk = if self.config_path.exists() {
            Config::load(&self.config_path)
        } else {
            Ok(self.config.clone())
        };
        match on_disk {
            Ok(mut on_disk) => {
                on_disk.last_save_as_dir = dir.to_path_buf();
                match on_disk.save(&self.config_path) {
                    Ok(()) => info!("last_save_as_dir = {}", dir.display()),
                    Err(e) => error!("cannot save last_save_as_dir: {e:#}"),
                }
            }
            Err(e) => warn!("last_save_as_dir not saved, the config file does not parse: {e:#}"),
        }
    }

    /// Saves the whole virtual screen to `save_dir/debug_capture.png`.
    fn debug_dump_capture(&self) {
        let Some(capture) = capture_or_report() else { return };
        info!(
            "captured the virtual screen: origin ({}, {}), {}x{}",
            capture.x0, capture.y0, capture.width, capture.height
        );
        let Some(image) = util::crop_rgba(&capture, capture.rect()) else { return };
        let dir = self.config.save_dir.clone();
        thread::spawn(move || {
            let path = dir.join("debug_capture.png");
            let result = fs::create_dir_all(&dir)
                .with_context(|| format!("cannot create {}", dir.display()))
                .and_then(|()| {
                    image.save(&path).with_context(|| format!("cannot save {}", path.display()))
                });
            match result {
                Ok(()) => info!("debug capture saved to {}", path.display()),
                Err(e) => {
                    error!("{e:#}");
                    dialog::error(&format!("{e:#}"));
                }
            }
        });
    }
}

fn spawn_program(command: &mut Command) {
    if let Err(e) = command.spawn() {
        let program = command.get_program().to_string_lossy().into_owned();
        error!("cannot start {program}: {e}");
        dialog::error_async(format!("Could not start {program}: {e}"));
    }
}

fn capture_or_report() -> Option<Capture> {
    capture::capture_virtual_screen()
        .map_err(|e| {
            error!("capture failed: {e:#}");
            dialog::error_async(format!("Capture failed: {e:#}"));
        })
        .ok()
}

/// Hands heap memory that has been freed back to Windows. Parsing the overlay font makes ~20 MB
/// of small allocations; once the font is dropped, the heap would otherwise keep about 5 MB of
/// that committed for the rest of the run.
fn release_freed_memory() {
    use windows::Win32::System::Memory::{HeapOptimizeResources, HeapSetInformation};
    use windows::Win32::System::SystemServices::{
        HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION, HEAP_OPTIMIZE_RESOURCES_INFORMATION,
    };

    let info = HEAP_OPTIMIZE_RESOURCES_INFORMATION {
        Version: HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION,
        Flags: 0,
    };
    // Without a heap handle this applies to every heap of the process.
    let result = unsafe {
        HeapSetInformation(
            None,
            HeapOptimizeResources,
            Some(&info as *const HEAP_OPTIMIZE_RESOURCES_INFORMATION as *const core::ffi::c_void),
            std::mem::size_of_val(&info),
        )
    };
    if let Err(e) = result {
        warn!("cannot release freed heap memory: {e}");
    }
}

/// Monitor rectangle in physical virtual-screen pixels.
fn monitor_rect(monitor: &MonitorHandle) -> Rect {
    let (pos, size) = (monitor.position(), monitor.size());
    Rect::new(pos.x, pos.y, size.width as i32, size.height as i32)
}

fn monitor_rects(event_loop: &ActiveEventLoop) -> Vec<Rect> {
    event_loop.available_monitors().map(|m| monitor_rect(&m)).collect()
}

/// The monitor under the mouse cursor, else the primary one.
fn cursor_monitor(event_loop: &ActiveEventLoop) -> Option<Rect> {
    let monitors = monitor_rects(event_loop);
    capture::cursor_position()
        .and_then(|point| util::find_containing(&monitors, point))
        .map(|i| monitors[i])
        .or_else(|| event_loop.primary_monitor().map(|m| monitor_rect(&m)))
}

/// `cursor_monitor` in capture coordinates; the whole capture if there is no monitor at all.
fn cursor_monitor_in(event_loop: &ActiveEventLoop, capture: &Capture) -> Rect {
    cursor_monitor(event_loop).map_or(capture.rect(), |monitor| capture.screen_to_capture(monitor))
}

fn log_monitors(event_loop: &ActiveEventLoop) {
    for (i, monitor) in event_loop.available_monitors().enumerate() {
        let rect = monitor_rect(&monitor);
        info!(
            "monitor {i} {:?}: position ({}, {}), size {}x{} physical px, scale {}",
            monitor.name().unwrap_or_default(),
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            monitor.scale_factor()
        );
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.init(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::HotkeyRegion => self.capture_region(event_loop),
            UserEvent::HotkeyFullscreen => self.capture_fullscreen(event_loop),
            UserEvent::HotkeyWindow => self.capture_window(event_loop, false),
            UserEvent::TrayMenu(id) => self.on_menu(event_loop, id),
            UserEvent::TrayDoubleClick => self.open_folder(),
            UserEvent::DelayedCapture(CaptureKind::Region) => self.capture_region(event_loop),
            UserEvent::DelayedCapture(CaptureKind::Fullscreen) => self.capture_fullscreen(event_loop),
            UserEvent::DelayedCapture(CaptureKind::Window) => self.capture_window(event_loop, true),
            UserEvent::WorkerDone(result) => self.on_worker_done(result),
            UserEvent::SettingsSaved(saved) => self.on_settings_saved(saved),
            UserEvent::SettingsClosed => self.on_settings_closed(),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.on_overlay_event(id, event);
    }
}
