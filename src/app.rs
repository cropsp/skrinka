use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Context;
use log::{error, info};
use tray_icon::menu::{MenuEvent, MenuId};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

use crate::capture::{self, Capture};
use crate::config::{Config, FullscreenMode};
use crate::dialog;
use crate::hotkeys::Hotkeys;
use crate::output::{self, Action, WorkerResult};
use crate::tray::Tray;
use crate::util::{self, Rect};

/// `EventLoopProxy` is `Send` but not `Sync` on Windows, while the tauri-apps event handlers
/// must be `Send + Sync`, hence the mutex.
pub type SharedProxy = Arc<Mutex<EventLoopProxy<UserEvent>>>;

#[derive(Debug)]
pub enum UserEvent {
    HotkeyRegion,
    HotkeyFullscreen,
    TrayMenu(MenuId),
    WorkerDone(WorkerResult),
}

pub fn send_event(proxy: &SharedProxy, event: UserEvent) {
    let proxy = proxy.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Fails only once the event loop is gone, and then nobody is listening anyway.
    let _ = proxy.send_event(event);
}

pub struct App {
    proxy: SharedProxy,
    config: Config,
    initialized: bool,
    tray: Option<Tray>,
    hotkeys: Option<Hotkeys>,
    fatal_error: Option<String>,
}

impl App {
    pub fn new(config: Config, proxy: SharedProxy) -> App {
        App { proxy, config, initialized: false, tray: None, hotkeys: None, fatal_error: None }
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

        // muda keeps only the first handler it is given, so this runs exactly once.
        let proxy = self.proxy.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            send_event(&proxy, UserEvent::TrayMenu(e.id));
        }));

        match Hotkeys::new(self.proxy.clone()) {
            Ok(mut hotkeys) => {
                if let Some(problems) = hotkeys.register(&self.config) {
                    dialog::warning_async(problems);
                }
                self.hotkeys = Some(hotkeys);
            }
            Err(e) => {
                error!("cannot set up global hotkeys: {e:#}");
                dialog::error_async(format!("Could not set up the global hotkeys: {e:#}"));
            }
        }

        log_monitors(event_loop);
        info!("ready; screenshots go to {}", self.config.save_dir.display());
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: MenuId) {
        let Some(tray) = &self.tray else { return };
        if id == *tray.debug_dump.id() {
            self.debug_dump_capture();
        } else if id == *tray.quit.id() {
            info!("quit from the tray menu");
            event_loop.exit();
        }
    }

    fn capture_fullscreen(&mut self, event_loop: &ActiveEventLoop) {
        let Some(capture) = capture_or_report() else { return };
        let rect = match self.config.fullscreen_mode {
            FullscreenMode::AllMonitors => capture.rect(),
            FullscreenMode::CursorMonitor => cursor_monitor(event_loop)
                .map_or(capture.rect(), |monitor| capture.screen_to_capture(monitor)),
        };
        let Some(image) = util::crop_rgba(&capture, rect) else {
            error!("full-screen capture: {rect:?} is outside the capture");
            return;
        };
        info!("full-screen capture of {rect:?}");
        output::spawn(image, Action::ClipboardAndAutoSave, self.config.clone(), self.proxy.clone());
    }

    fn on_worker_done(&mut self, result: WorkerResult) {
        match (&result.error, &result.saved_path) {
            (Some(_), _) => info!("{:?} finished with errors", result.action),
            (None, Some(path)) => info!("{:?} finished: {}", result.action, path.display()),
            (None, None) => info!("{:?} finished", result.action),
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

fn capture_or_report() -> Option<Capture> {
    capture::capture_virtual_screen()
        .map_err(|e| {
            error!("capture failed: {e:#}");
            dialog::error_async(format!("Capture failed: {e:#}"));
        })
        .ok()
}

/// Monitor rectangle in physical virtual-screen pixels.
fn monitor_rect(monitor: &MonitorHandle) -> Rect {
    let (pos, size) = (monitor.position(), monitor.size());
    Rect::new(pos.x, pos.y, size.width as i32, size.height as i32)
}

/// The monitor under the mouse cursor, else the primary one.
fn cursor_monitor(event_loop: &ActiveEventLoop) -> Option<Rect> {
    let monitors: Vec<Rect> = event_loop.available_monitors().map(|m| monitor_rect(&m)).collect();
    capture::cursor_position()
        .and_then(|point| util::find_containing(&monitors, point))
        .map(|i| monitors[i])
        .or_else(|| event_loop.primary_monitor().map(|m| monitor_rect(&m)))
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
            UserEvent::HotkeyRegion => info!("region hotkey pressed; the overlay arrives in step 4"),
            UserEvent::HotkeyFullscreen => self.capture_fullscreen(event_loop),
            UserEvent::TrayMenu(id) => self.on_menu(event_loop, id),
            UserEvent::WorkerDone(result) => self.on_worker_done(result),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}
