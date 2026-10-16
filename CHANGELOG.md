# Changelog

All notable changes to Skrinka are listed here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- MIT `LICENSE` (it covers every version, including 0.1.0 and 0.2.0), `THIRD-PARTY-LICENSES.html`
  and this changelog.
- Build number, commit and build date in the binary: the log starts with
  `Skrinka 0.3.0 (Build N · abc1234 · YYYY-MM-DD) starting`; the tray tooltip shows the version.
- `skrinka.exe` has an icon and version information (Explorer → Properties → Details).
- Settings window ("Settings…" in the tray menu): hotkeys (click Change and press the keys;
  Backspace disables a hotkey), screenshot folder, file name template, full-screen and window
  capture modes, notifications and "Run on startup", with the version and build number at the
  top. It opens on the monitor under the mouse and checks the values before saving; saved
  settings apply at once. The global hotkeys are paused while it is open.
- An empty hotkey in `config.toml` disables that hotkey.

### Changed

- The tray menu is shorter: "Open config file", "Reload config" and "Run on startup" are in the
  Settings window now. `config.toml` can still be edited by hand; edits apply at the next start
  or when Settings is opened and saved.
- The design specs moved to `docs/design/`.

## [0.2.0] - 2026-10-16

### Added

- Region overlay: drag inside the selection to move it, drag an edge or a corner to resize it
  (dragging past the opposite side flips it); handles on the corners and edge midpoints; the
  cursor shows what a press would do.
- Arrow keys move the selection by 1 px, `Shift`+arrows by 10 px.
- Capture the active window with `Shift+Alt+3` (`hotkey_window`), as drawn with its title bar
  (`window_capture = 'visible'`) or its contents only (`'client'`). A maximized window is cut to
  its monitor; with no sensible window in front, the monitor under the cursor is captured.
- "Capture window" in the tray menu takes the window you were working in.
- A notification after a screenshot is saved, with the file name and folder (`notifications`:
  `saves`, `all` or `off`).

## [0.1.0] - 2026-10-16

### Added

- Tray app with a single-instance guard and a log file in `%APPDATA%\Skrinka\`.
- `Shift+Alt+1` captures the monitor under the cursor (or all monitors, `fullscreen_mode`) to the
  clipboard and a PNG file.
- `Shift+Alt+2` region capture: the screens freeze and dim, drag a rectangle; `Enter` copies and
  saves, `Ctrl+C` copies only, `Ctrl+S` opens "Save as" in the last used folder, `Esc` or a right
  click cancels. Keys work with any keyboard layout.
- File names from a strftime template; characters Windows does not allow are replaced, and
  ` (2)`, ` (3)`, … keeps two screenshots from overwriting each other.
- `config.toml` with defaults, "Open config file", "Reload config", "Run on startup" and
  "Open screenshots folder" in the tray menu.

[Unreleased]: https://github.com/cropsp/skrinka/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/cropsp/skrinka/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cropsp/skrinka/releases/tag/v0.1.0
