use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info};

use crate::app::{send_event, SharedProxy, UserEvent};
use crate::config::{Config, DEFAULT_HOTKEY_FULLSCREEN, DEFAULT_HOTKEY_REGION, DEFAULT_HOTKEY_WINDOW};

#[derive(Debug, Clone, Copy, Default)]
struct Ids {
    region: Option<u32>,
    fullscreen: Option<u32>,
    window: Option<u32>,
}

pub struct Hotkeys {
    /// Dropping the manager unregisters the hotkeys, so it lives as long as `App`.
    manager: GlobalHotKeyManager,
    registered: Vec<HotKey>,
    /// global-hotkey keeps only the first event handler it is given, so re-registering after
    /// "Reload config" changes these shared ids instead of the handler.
    ids: Arc<Mutex<Ids>>,
    /// For the tray menu: the hotkey in effect, or why there is none.
    pub region_label: String,
    pub fullscreen_label: String,
    pub window_label: String,
}

impl Hotkeys {
    pub fn new(proxy: SharedProxy) -> anyhow::Result<Hotkeys> {
        let manager = GlobalHotKeyManager::new()?;
        let ids = Arc::new(Mutex::new(Ids::default()));
        let handler_ids = ids.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            // Every hotkey fires again on release.
            if e.state != HotKeyState::Pressed {
                return;
            }
            let ids = *handler_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let event = if Some(e.id) == ids.region {
                UserEvent::HotkeyRegion
            } else if Some(e.id) == ids.fullscreen {
                UserEvent::HotkeyFullscreen
            } else if Some(e.id) == ids.window {
                UserEvent::HotkeyWindow
            } else {
                return;
            };
            send_event(&proxy, event);
        }));
        Ok(Hotkeys {
            manager,
            registered: Vec::new(),
            ids,
            region_label: String::new(),
            fullscreen_label: String::new(),
            window_label: String::new(),
        })
    }

    /// Registers the hotkeys from the config, replacing the previous ones. Returns a message
    /// for the user if a hotkey could not be registered.
    pub fn register(&mut self, config: &Config) -> Option<String> {
        if !self.registered.is_empty() {
            if let Err(e) = self.manager.unregister_all(&self.registered) {
                error!("cannot unregister the old hotkeys: {e}");
            }
            self.registered.clear();
        }
        let mut problems = Vec::new();
        let (region, region_label) =
            self.register_one("hotkey_region", &config.hotkey_region, DEFAULT_HOTKEY_REGION, &mut problems);
        let (fullscreen, fullscreen_label) = self.register_one(
            "hotkey_fullscreen",
            &config.hotkey_fullscreen,
            DEFAULT_HOTKEY_FULLSCREEN,
            &mut problems,
        );
        let (window, window_label) =
            self.register_one("hotkey_window", &config.hotkey_window, DEFAULT_HOTKEY_WINDOW, &mut problems);
        *self.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Ids { region, fullscreen, window };
        self.region_label = region_label;
        self.fullscreen_label = fullscreen_label;
        self.window_label = window_label;
        (!problems.is_empty()).then(|| problems.join("\n\n"))
    }

    /// Returns the id if registered, and the label for the tray menu.
    fn register_one(
        &mut self,
        key: &str,
        text: &str,
        default: &str,
        problems: &mut Vec<String>,
    ) -> (Option<u32>, String) {
        let (hotkey, text) = match text.parse::<HotKey>() {
            Ok(hotkey) => (hotkey, text),
            Err(e) => {
                error!("{key} = {text:?} is not a valid hotkey ({e}); using {default}");
                (default.parse().expect("the default hotkeys are valid"), default)
            }
        };
        match self.manager.register(hotkey) {
            Ok(()) => {
                info!("registered {key} = {text}");
                self.registered.push(hotkey);
                (Some(hotkey.id()), text.to_owned())
            }
            Err(e) => {
                error!("cannot register {key} = {text}: {e}");
                problems.push(format!(
                    "Could not register hotkey {text} — it is probably used by another program. \
                     Change it in config.toml."
                ));
                (None, format!("{text} (not available)"))
            }
        }
    }
}
