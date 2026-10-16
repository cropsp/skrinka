//! What the Settings window edits, and the checks before it saves. Pure, so it is unit-tested.

use std::path::PathBuf;

use global_hotkey::hotkey::HotKey;

use crate::config::{Config, FullscreenMode, Notifications, WindowCapture};
use crate::output;
use crate::strings;

/// Combo box entries, in the order of `strings::SETTINGS_*_CHOICES`.
pub const FULLSCREEN_MODES: [FullscreenMode; 2] = [FullscreenMode::CursorMonitor, FullscreenMode::AllMonitors];
pub const WINDOW_CAPTURES: [WindowCapture; 2] = [WindowCapture::Visible, WindowCapture::Client];
pub const NOTIFICATIONS: [Notifications; 3] = [Notifications::Saves, Notifications::All, Notifications::Off];

/// The contents of the window's controls.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub save_dir: String,
    pub filename_format: String,
    /// Region, full screen, window; stored form (`Super`, not `Win`); empty = disabled.
    pub hotkeys: [String; 3],
    pub fullscreen_mode: usize,
    pub window_capture: usize,
    pub notifications: usize,
    pub autostart: bool,
}

/// What the window saves.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub config: Config,
    pub autostart: bool,
}

fn index_of<T: PartialEq>(list: &[T], value: T) -> usize {
    list.iter().position(|v| *v == value).unwrap_or(0)
}

impl Form {
    pub fn from_config(config: &Config, autostart: bool) -> Form {
        Form {
            save_dir: config.save_dir.display().to_string(),
            filename_format: config.filename_format.clone(),
            hotkeys: [config.hotkey_region.clone(), config.hotkey_fullscreen.clone(), config.hotkey_window.clone()],
            fullscreen_mode: index_of(&FULLSCREEN_MODES, config.fullscreen_mode),
            window_capture: index_of(&WINDOW_CAPTURES, config.window_capture),
            notifications: index_of(&NOTIFICATIONS, config.notifications),
            autostart,
        }
    }

    /// The config to save, starting from `base` (which keeps `last_save_as_dir`), or the reason
    /// the form cannot be saved.
    pub fn validate(&self, base: &Config) -> Result<Saved, String> {
        let save_dir = self.save_dir.trim();
        if save_dir.is_empty() {
            return Err(strings::INVALID_FOLDER_EMPTY.to_owned());
        }
        let template = self.filename_format.trim();
        if template.is_empty() {
            return Err(strings::INVALID_TEMPLATE_EMPTY.to_owned());
        }
        if !output::is_valid_template(template) {
            return Err(strings::invalid_template(template));
        }
        let labels = strings::SETTINGS_HOTKEY_LABELS;
        let mut parsed: Vec<(usize, HotKey)> = Vec::new();
        for (i, text) in self.hotkeys.iter().enumerate() {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let hotkey = text.parse::<HotKey>().map_err(|_| strings::invalid_hotkey(labels[i], text))?;
            if let Some((first, _)) = parsed.iter().find(|(_, other)| *other == hotkey) {
                return Err(strings::duplicate_hotkey(labels[*first], labels[i]));
            }
            parsed.push((i, hotkey));
        }
        let pick = |list_len: usize, i: usize| i.min(list_len - 1);
        let [region, fullscreen, window] = self.hotkeys.clone().map(|h| h.trim().to_owned());
        let config = Config {
            save_dir: PathBuf::from(save_dir),
            filename_format: template.to_owned(),
            hotkey_region: region,
            hotkey_fullscreen: fullscreen,
            hotkey_window: window,
            fullscreen_mode: FULLSCREEN_MODES[pick(FULLSCREEN_MODES.len(), self.fullscreen_mode)],
            window_capture: WINDOW_CAPTURES[pick(WINDOW_CAPTURES.len(), self.window_capture)],
            notifications: NOTIFICATIONS[pick(NOTIFICATIONS.len(), self.notifications)],
            ..base.clone()
        };
        Ok(Saved { config, autostart: self.autostart })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Config {
        Config {
            save_dir: PathBuf::from(r"C:\Shots"),
            last_save_as_dir: PathBuf::from(r"C:\Elsewhere"),
            ..Config::default()
        }
    }

    #[test]
    fn unchanged_form_gives_the_same_config() {
        let base = base();
        let saved = Form::from_config(&base, true).validate(&base).unwrap();
        assert_eq!(saved, Saved { config: base, autostart: true });
    }

    #[test]
    fn combo_indexes_round_trip() {
        let config = Config {
            fullscreen_mode: FullscreenMode::AllMonitors,
            window_capture: WindowCapture::Client,
            notifications: Notifications::Off,
            ..base()
        };
        let form = Form::from_config(&config, false);
        assert_eq!((form.fullscreen_mode, form.window_capture, form.notifications), (1, 1, 2));
        assert_eq!(form.validate(&base()).unwrap().config, config);
    }

    #[test]
    fn edits_are_applied_and_last_save_as_dir_kept() {
        let mut form = Form::from_config(&base(), false);
        form.save_dir = r"  D:\New folder ".to_owned();
        form.filename_format = "Shot %H-%M".to_owned();
        form.hotkeys[0] = "Ctrl+Shift+F9".to_owned();
        form.hotkeys[2] = String::new();
        form.notifications = 1;
        let config = form.validate(&base()).unwrap().config;
        assert_eq!(config.save_dir, PathBuf::from(r"D:\New folder"));
        assert_eq!(config.filename_format, "Shot %H-%M");
        assert_eq!(config.hotkey_region, "Ctrl+Shift+F9");
        assert_eq!(config.hotkey_window, "");
        assert_eq!(config.notifications, Notifications::All);
        assert_eq!(config.last_save_as_dir, PathBuf::from(r"C:\Elsewhere"));
    }

    #[test]
    fn empty_folder_and_template_are_rejected() {
        let mut form = Form::from_config(&base(), false);
        form.save_dir = "   ".to_owned();
        assert_eq!(form.validate(&base()), Err(strings::INVALID_FOLDER_EMPTY.to_owned()));
        let mut form = Form::from_config(&base(), false);
        form.filename_format = String::new();
        assert_eq!(form.validate(&base()), Err(strings::INVALID_TEMPLATE_EMPTY.to_owned()));
    }

    #[test]
    fn invalid_template_is_rejected() {
        let mut form = Form::from_config(&base(), false);
        form.filename_format = "%Q%".to_owned();
        assert_eq!(form.validate(&base()), Err(strings::invalid_template("%Q%")));
    }

    #[test]
    fn invalid_hotkey_is_rejected() {
        let mut form = Form::from_config(&base(), false);
        form.hotkeys[1] = "Win+S".to_owned();
        assert_eq!(form.validate(&base()), Err(strings::invalid_hotkey("Capture full screen", "Win+S")));
    }

    #[test]
    fn duplicate_hotkeys_are_rejected_in_any_modifier_order() {
        let mut form = Form::from_config(&base(), false);
        form.hotkeys[2] = "Alt+Shift+2".to_owned();
        assert_eq!(form.validate(&base()), Err(strings::duplicate_hotkey("Capture region", "Capture window")));
    }

    #[test]
    fn several_disabled_hotkeys_are_not_duplicates() {
        let mut form = Form::from_config(&base(), false);
        form.hotkeys = [String::new(), String::new(), "PrintScreen".to_owned()];
        let config = form.validate(&base()).unwrap().config;
        assert_eq!((config.hotkey_region.as_str(), config.hotkey_window.as_str()), ("", "PrintScreen"));
    }
}
