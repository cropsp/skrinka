use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Context;
use log::{error, info};
use tray_icon::menu::{MenuEvent, MenuId};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::capture;
use crate::config::Config;
use crate::dialog;
use crate::tray::Tray;

/// `EventLoopProxy` is `Send` but not `Sync` on Windows, while the tauri-apps event handlers
/// must be `Send + Sync`, hence the mutex.
pub type SharedProxy = Arc<Mutex<EventLoopProxy<UserEvent>>>;

#[derive(Debug)]
pub enum UserEvent {
    TrayMenu(MenuId),
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
    fatal_error: Option<String>,
}

impl App {
    pub fn new(config: Config, proxy: SharedProxy) -> App {
        App { proxy, config, initialized: false, tray: None, fatal_error: None }
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

    /// Saves the whole virtual screen to `save_dir/debug_capture.png`.
    fn debug_dump_capture(&self) {
        let capture = match capture::capture_virtual_screen() {
            Ok(capture) => capture,
            Err(e) => {
                error!("capture failed: {e:#}");
                dialog::error_async(format!("Capture failed: {e:#}"));
                return;
            }
        };
        info!(
            "captured the virtual screen: origin ({}, {}), {}x{}",
            capture.x0, capture.y0, capture.width, capture.height
        );
        let dir = self.config.save_dir.clone();
        thread::spawn(move || {
            let path = dir.join("debug_capture.png");
            let result = fs::create_dir_all(&dir)
                .with_context(|| format!("cannot create {}", dir.display()))
                .and_then(|()| {
                    capture
                        .to_rgba_image()
                        .save(&path)
                        .with_context(|| format!("cannot save {}", path.display()))
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

fn log_monitors(event_loop: &ActiveEventLoop) {
    for (i, monitor) in event_loop.available_monitors().enumerate() {
        let (pos, size) = (monitor.position(), monitor.size());
        info!(
            "monitor {i} {:?}: position ({}, {}), size {}x{} physical px, scale {}",
            monitor.name().unwrap_or_default(),
            pos.x,
            pos.y,
            size.width,
            size.height,
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
            UserEvent::TrayMenu(id) => self.on_menu(event_loop, id),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}
