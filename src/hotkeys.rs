use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info};

use crate::app::{send_event, SharedProxy, UserEvent};
use crate::config::{Config, DEFAULT_HOTKEY_FULLSCREEN, DEFAULT_HOTKEY_REGION, DEFAULT_HOTKEY_WINDOW};
use crate::settings::hotkey_text;
use crate::strings;

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
    /// global-hotkey keeps only the first event handler it is given, so re-registering (around
    /// the Settings window) changes these shared ids instead of the handler.
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
        self.unregister_all();
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

    /// Unregisters every hotkey, so the keys reach other programs (and the Settings window,
    /// which records them). The menu labels stay as they are.
    pub fn unregister_all(&mut self) {
        if !self.registered.is_empty() {
            match self.manager.unregister_all(&self.registered) {
                Ok(()) => info!("hotkeys unregistered"),
                Err(e) => error!("cannot unregister the hotkeys: {e}"),
            }
            self.registered.clear();
        }
        *self.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Ids::default();
    }

    /// Returns the id if registered, and the label for the tray menu. An empty `text` means
    /// the hotkey is disabled.
    fn register_one(
        &mut self,
        key: &str,
        text: &str,
        default: &str,
        problems: &mut Vec<String>,
    ) -> (Option<u32>, String) {
        let text = text.trim();
        if text.is_empty() {
            info!("{key} disabled");
            return (None, strings::HOTKEY_DISABLED.to_owned());
        }
        let (hotkey, text) = match text.parse::<HotKey>() {
            Ok(hotkey) => (hotkey, text),
            Err(e) => {
                error!("{key} = {text:?} is not a valid hotkey ({e}); using {default}");
                (default.parse().expect("the default hotkeys are valid"), default)
            }
        };
        let shown = hotkey_text::display(text);
        match self.manager.register(hotkey) {
            Ok(()) => {
                info!("registered {key} = {text}");
                self.registered.push(hotkey);
                (Some(hotkey.id()), shown)
            }
            Err(e) => {
                error!("cannot register {key} = {text}: {e}");
                problems.push(strings::hotkey_register_failed(&shown));
                (None, strings::hotkey_not_available(&shown))
            }
        }
    }
}
