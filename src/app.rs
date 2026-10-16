use std::sync::{Arc, Mutex};

use log::{error, info};
use tray_icon::menu::{MenuEvent, MenuId};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::config::Config;
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

        info!("ready; screenshots go to {}", self.config.save_dir.display());
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: MenuId) {
        let Some(tray) = &self.tray else { return };
        if id == *tray.quit.id() {
            info!("quit from the tray menu");
            event_loop.exit();
        }
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
