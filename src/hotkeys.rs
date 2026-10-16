use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info};

use crate::app::{send_event, SharedProxy, UserEvent};
use crate::config::{Config, DEFAULT_HOTKEY_FULLSCREEN, DEFAULT_HOTKEY_REGION};

#[derive(Debug, Clone, Copy, Default)]
struct Ids {
    region: Option<u32>,
    fullscreen: Option<u32>,
}

pub struct Hotkeys {
    /// Dropping the manager unregisters the hotkeys, so it lives as long as `App`.
    manager: GlobalHotKeyManager,
    registered: Vec<HotKey>,
    /// global-hotkey keeps only the first event handler it is given, so re-registering after
    /// "Reload config" changes these shared ids instead of the handler.
    ids: Arc<Mutex<Ids>>,
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
            } else {
                return;
            };
            send_event(&proxy, event);
        }));
        Ok(Hotkeys { manager, registered: Vec::new(), ids })
    }

    /// Registers both hotkeys from the config, replacing the previous ones. Returns a message
    /// for the user if a hotkey could not be registered.
    pub fn register(&mut self, config: &Config) -> Option<String> {
        if !self.registered.is_empty() {
            if let Err(e) = self.manager.unregister_all(&self.registered) {
                error!("cannot unregister the old hotkeys: {e}");
            }
            self.registered.clear();
        }
        let mut problems = Vec::new();
        let region =
            self.register_one("hotkey_region", &config.hotkey_region, DEFAULT_HOTKEY_REGION, &mut problems);
        let fullscreen = self.register_one(
            "hotkey_fullscreen",
            &config.hotkey_fullscreen,
            DEFAULT_HOTKEY_FULLSCREEN,
            &mut problems,
        );
        *self.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Ids { region, fullscreen };
        (!problems.is_empty()).then(|| problems.join("\n\n"))
    }

    fn register_one(
        &mut self,
        key: &str,
        text: &str,
        default: &str,
        problems: &mut Vec<String>,
    ) -> Option<u32> {
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
                Some(hotkey.id())
            }
            Err(e) => {
                error!("cannot register {key} = {text}: {e}");
                problems.push(format!(
                    "Could not register hotkey {text} — it is probably used by another program. \
                     Change it in config.toml."
                ));
                None
            }
        }
    }
}
