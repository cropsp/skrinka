//! User-visible text, in one place so a translation (Ukrainian is planned) touches one file.
//! Log lines are not here: they stay in English.

// Tray menu.
pub const MENU_CAPTURE_REGION: &str = "Capture region";
pub const MENU_CAPTURE_FULLSCREEN: &str = "Capture full screen";
pub const MENU_CAPTURE_WINDOW: &str = "Capture window";
pub const MENU_OPEN_FOLDER: &str = "Open screenshots folder";
pub const MENU_SETTINGS: &str = "Settings…";
pub const MENU_DEBUG_DUMP: &str = "Debug: dump capture";
pub const MENU_QUIT: &str = "Quit";
/// Next to a capture item whose hotkey is empty.
pub const HOTKEY_DISABLED: &str = "disabled";

pub fn hotkey_not_available(hotkey: &str) -> String {
    format!("{hotkey} (not available)")
}

pub fn hotkey_register_failed(hotkey: &str) -> String {
    format!("Could not register hotkey {hotkey} — it is probably used by another program. Change it in Settings.")
}

// Notifications.
pub const APP_NAME: &str = "Skrinka";
pub const NOTIFY_SAVED: &str = "Screenshot saved";
pub const NOTIFY_COPIED: &str = "Copied to clipboard";
pub const NOTIFY_SETTINGS_SAVED: &str = "Settings saved and applied.";

// Region overlay.
pub const OVERLAY_HELP: &str = "Drag to select  ·  Drag inside to move, edges to resize  ·  Arrows: nudge  ·  \
                                Enter: copy + save  ·  Ctrl+S: save as  ·  Ctrl+C: copy  ·  Esc: cancel";

// Settings window. No `&` accelerators: Alt+letter must reach the hotkey fields.
pub const SETTINGS_TITLE: &str = "Skrinka — Settings";
pub const SETTINGS_HOTKEYS: &str = "Hotkeys";
pub const SETTINGS_HOTKEY_LABELS: [&str; 3] = ["Capture region", "Capture full screen", "Capture window"];
pub const SETTINGS_CHANGE: &str = "Change";
pub const SETTINGS_PRESS_KEYS: &str = "Press keys…";
pub const SETTINGS_SAVING: &str = "Saving";
pub const SETTINGS_FOLDER: &str = "Folder";
pub const SETTINGS_BROWSE: &str = "Browse…";
pub const SETTINGS_FILE_NAME: &str = "File name";
pub const SETTINGS_STRFTIME_HINT: &str = "strftime codes: %Y year, %m month, %d day, %H:%M:%S time";
pub const SETTINGS_CAPTURE: &str = "Capture";
pub const SETTINGS_FULLSCREEN: &str = "Full screen captures";
pub const SETTINGS_FULLSCREEN_CHOICES: [&str; 2] = ["Monitor under the cursor", "All monitors"];
pub const SETTINGS_WINDOW: &str = "Window captures";
pub const SETTINGS_WINDOW_CHOICES: [&str; 2] = ["Visible window (with title bar)", "Window contents only"];
pub const SETTINGS_NOTIFICATIONS: &str = "Notifications";
pub const SETTINGS_NOTIFICATIONS_CHOICES: [&str; 3] = ["After saving a file", "After every capture", "Off"];
pub const SETTINGS_AUTOSTART: &str = "Run on startup";
pub const SETTINGS_LINK: &str = r#"<a href="https://github.com/cropsp/skrinka">github.com/cropsp/skrinka</a>"#;
pub const SETTINGS_SAVE: &str = "Save";
pub const SETTINGS_CANCEL: &str = "Cancel";
pub const SETTINGS_PICK_FOLDER: &str = "Folder for screenshots";

// Settings validation.
pub const INVALID_FOLDER_EMPTY: &str = "Choose a folder for the screenshots.";
pub const INVALID_TEMPLATE_EMPTY: &str = "Enter a file name template.";

pub fn invalid_template(template: &str) -> String {
    format!("\"{template}\" is not a valid file name template. Check the % codes.")
}

pub fn invalid_hotkey(label: &str, hotkey: &str) -> String {
    format!("{label}: \"{hotkey}\" is not a valid hotkey. Click Change and press a new one.")
}

pub fn duplicate_hotkey(first: &str, second: &str) -> String {
    format!("{first} and {second} have the same hotkey.")
}
