use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use log::info;
use serde::{Deserialize, Serialize};

const HEADER: &str = r#"# Skrinka configuration. Use "Settings…" in the tray menu to change it. Edits made here apply
# at the next start, or when Settings is opened and saved.
#
# save_dir           folder for auto-saved screenshots
# filename_format    strftime template, e.g. 'Screenshot %Y-%m-%d %H-%M-%S'; ".png" is added automatically
# hotkey_region      region capture hotkey, e.g. 'Shift+Alt+2'; modifiers Ctrl, Shift, Alt, Super (the
#                    Windows key) plus one key, joined with '+'; '' disables the hotkey
# hotkey_fullscreen  full-screen capture hotkey, e.g. 'Shift+Alt+1'
# hotkey_window      active window capture hotkey, e.g. 'Shift+Alt+3'
# fullscreen_mode    'cursor_monitor' (the monitor under the mouse) or 'all_monitors'
# window_capture     'visible' (the window as you see it, title bar included) or 'client' (its contents only)
# notifications      'saves' (after a screenshot is saved to a file), 'all' (also after Ctrl+C) or 'off'
# last_save_as_dir   folder of the last "Save as" (maintained by the program)
"#;

pub const DEFAULT_FILENAME_FORMAT: &str = "Screenshot %Y-%m-%d %H-%M-%S";
pub const DEFAULT_HOTKEY_REGION: &str = "Shift+Alt+2";
pub const DEFAULT_HOTKEY_FULLSCREEN: &str = "Shift+Alt+1";
pub const DEFAULT_HOTKEY_WINDOW: &str = "Shift+Alt+3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenMode {
    CursorMonitor,
    AllMonitors,
}

/// What "capture window" takes of the active window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowCapture {
    /// The window as drawn: frame and title bar, without the invisible resize borders.
    Visible,
    /// The client area only, without the title bar.
    Client,
}

/// When a tray notification appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Notifications {
    /// After a screenshot is saved to a file.
    Saves,
    /// Also after copying to the clipboard only.
    All,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub save_dir: PathBuf,
    pub filename_format: String,
    pub hotkey_region: String,
    pub hotkey_fullscreen: String,
    pub hotkey_window: String,
    pub fullscreen_mode: FullscreenMode,
    pub window_capture: WindowCapture,
    pub notifications: Notifications,
    /// Empty until the first "Save as".
    pub last_save_as_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let pictures = dirs::picture_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
            .unwrap_or_else(|| PathBuf::from("."));
        Config {
            save_dir: pictures.join("Screenshots"),
            filename_format: DEFAULT_FILENAME_FORMAT.to_owned(),
            hotkey_region: DEFAULT_HOTKEY_REGION.to_owned(),
            hotkey_fullscreen: DEFAULT_HOTKEY_FULLSCREEN.to_owned(),
            hotkey_window: DEFAULT_HOTKEY_WINDOW.to_owned(),
            fullscreen_mode: FullscreenMode::CursorMonitor,
            window_capture: WindowCapture::Visible,
            notifications: Notifications::Saves,
            last_save_as_dir: PathBuf::new(),
        }
    }
}

/// `%APPDATA%\Skrinka`
pub fn app_dir() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().context("cannot determine the %APPDATA% folder")?;
    Ok(base.join("Skrinka"))
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.join("config.toml"))
}

impl Config {
    pub fn parse(text: &str) -> anyhow::Result<Config> {
        // Some editors save UTF-8 with a byte-order mark.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Ok(toml::from_str(text)?)
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(format!("{HEADER}\n{}", toml::to_string_pretty(self)?))
    }

    pub fn load(path: &Path) -> anyhow::Result<Config> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        Config::parse(&text).with_context(|| format!("invalid config file {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        fs::write(path, self.to_toml()?)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// Loads the config, writing the defaults first if the file does not exist yet.
    pub fn load_or_create(path: &Path) -> anyhow::Result<Config> {
        if path.exists() {
            return Config::load(path);
        }
        let config = Config::default();
        config.save(path)?;
        info!("wrote default config to {}", path.display());
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_windows_paths() {
        let config = Config {
            save_dir: PathBuf::from(r"C:\Users\Тест\Pictures\Screenshots"),
            last_save_as_dir: PathBuf::from(r"D:\some dir\with 'quotes'"),
            fullscreen_mode: FullscreenMode::AllMonitors,
            window_capture: WindowCapture::Client,
            hotkey_window: "Ctrl+Shift+3".to_owned(),
            notifications: Notifications::All,
            ..Config::default()
        };
        let text = config.to_toml().unwrap();
        assert_eq!(Config::parse(&text).unwrap(), config);
    }

    #[test]
    fn literal_strings_with_backslashes_parse() {
        let text = "save_dir = 'C:\\Users\\me\\Pictures\\Screenshots'\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.save_dir, PathBuf::from(r"C:\Users\me\Pictures\Screenshots"));
    }

    #[test]
    fn missing_keys_get_defaults_and_unknown_keys_are_ignored() {
        let config = Config::parse("hotkey_region = 'Ctrl+Shift+4'\nsomething_else = 1\n").unwrap();
        assert_eq!(config.hotkey_region, "Ctrl+Shift+4");
        assert_eq!(config.filename_format, DEFAULT_FILENAME_FORMAT);
        assert_eq!(config.fullscreen_mode, FullscreenMode::CursorMonitor);
        assert_eq!(config.last_save_as_dir, PathBuf::new());
    }

    #[test]
    fn v1_config_file_gets_the_v2_defaults() {
        let v1 = "save_dir = 'C:\\Shots'\n\
                  filename_format = 'x'\n\
                  hotkey_region = 'Shift+Alt+2'\n\
                  hotkey_fullscreen = 'Shift+Alt+1'\n\
                  fullscreen_mode = 'cursor_monitor'\n\
                  last_save_as_dir = ''\n";
        let config = Config::parse(v1).unwrap();
        assert_eq!(config.hotkey_window, DEFAULT_HOTKEY_WINDOW);
        assert_eq!(config.window_capture, WindowCapture::Visible);
        assert_eq!(config.notifications, Notifications::Saves);
    }

    #[test]
    fn notifications_values() {
        assert_eq!(Config::parse("notifications = 'off'\n").unwrap().notifications, Notifications::Off);
        assert_eq!(Config::parse("notifications = 'all'\n").unwrap().notifications, Notifications::All);
        assert_eq!(Config::parse("notifications = 'saves'\n").unwrap().notifications, Notifications::Saves);
        assert!(Config::parse("notifications = 'yes'\n").is_err());
    }

    #[test]
    fn window_capture_values() {
        assert_eq!(Config::parse("window_capture = 'client'\n").unwrap().window_capture, WindowCapture::Client);
        assert_eq!(Config::parse("window_capture = 'visible'\n").unwrap().window_capture, WindowCapture::Visible);
        assert!(Config::parse("window_capture = 'whole'\n").is_err());
    }

    #[test]
    fn empty_hotkey_survives_a_round_trip() {
        let config = Config { hotkey_window: String::new(), ..Config::default() };
        let text = config.to_toml().unwrap();
        assert_eq!(Config::parse(&text).unwrap().hotkey_window, "");
    }

    #[test]
    fn byte_order_mark_is_accepted() {
        let config = Config::parse("\u{feff}fullscreen_mode = 'all_monitors'\n").unwrap();
        assert_eq!(config.fullscreen_mode, FullscreenMode::AllMonitors);
    }
}
