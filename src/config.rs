use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use log::info;
use serde::{Deserialize, Serialize};

const HEADER: &str = r#"# Skrinka configuration. Edit, then use "Reload config" in the tray menu.
#
# save_dir           folder for auto-saved screenshots
# filename_format    strftime template, e.g. 'Screenshot %Y-%m-%d %H-%M-%S'; ".png" is added automatically
# hotkey_region      region capture hotkey, e.g. 'Shift+Alt+2'
# hotkey_fullscreen  full-screen capture hotkey, e.g. 'Shift+Alt+1'
# fullscreen_mode    'cursor_monitor' (the monitor under the mouse) or 'all_monitors'
# last_save_as_dir   folder of the last "Save as" (maintained by the program)
"#;

pub const DEFAULT_FILENAME_FORMAT: &str = "Screenshot %Y-%m-%d %H-%M-%S";
pub const DEFAULT_HOTKEY_REGION: &str = "Shift+Alt+2";
pub const DEFAULT_HOTKEY_FULLSCREEN: &str = "Shift+Alt+1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullscreenMode {
    CursorMonitor,
    AllMonitors,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub save_dir: PathBuf,
    pub filename_format: String,
    pub hotkey_region: String,
    pub hotkey_fullscreen: String,
    pub fullscreen_mode: FullscreenMode,
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
            fullscreen_mode: FullscreenMode::CursorMonitor,
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
    fn byte_order_mark_is_accepted() {
        let config = Config::parse("\u{feff}fullscreen_mode = 'all_monitors'\n").unwrap();
        assert_eq!(config.fullscreen_mode, FullscreenMode::AllMonitors);
    }
}
