# Skrinka

A small screenshot tool for Windows 10/11, written in Rust. It replaces ScreenCloud for one job:
press a hotkey, drag a rectangle, and get the image on the clipboard and/or as a PNG file.

It lives in the system tray, has no main window, and never downloads anything.

## Building

Requirements: Rust (stable, `x86_64-pc-windows-msvc`) and the Visual Studio C++ Build Tools.

```
cargo build --release
```

The program is `target\release\skrinka.exe` (about 1.6 MB, a single file). Copy it anywhere you
like. If you move it after enabling "Run on startup", toggle that option off and on again so the
registry points at the new location.

Run the unit tests with `cargo test`.

Close ScreenCloud before starting Skrinka: both use `Shift+Alt+1` / `Shift+Alt+2` by default, and
whoever starts first gets the hotkeys. Skrinka shows a warning if it cannot register a hotkey.

## Using it

| Hotkey        | Action                                                                  |
|---------------|-------------------------------------------------------------------------|
| `Shift+Alt+1` | Capture the monitor under the mouse cursor → clipboard + PNG file        |
| `Shift+Alt+2` | Region capture: the screens freeze and dim, drag a rectangle with the mouse |

In the region overlay:

| Key                  | Action                                                             |
|----------------------|--------------------------------------------------------------------|
| `Enter`              | Copy to clipboard and save a PNG in `save_dir`                     |
| `Ctrl+C`             | Copy to clipboard only                                             |
| `Ctrl+S`             | "Save as" dialog (opens in the last "Save as" folder) + clipboard  |
| `Esc` / right click  | Cancel                                                             |

Drag again to replace the selection. Keys work with any keyboard layout, including Ukrainian.

Tray menu: Capture region, Capture full screen, Open screenshots folder, Open config file,
Reload config, Run on startup, Quit. Double-clicking the tray icon opens the screenshots folder.
Captures started from the menu wait 300 ms so the menu is not in the picture.

## Configuration

`%APPDATA%\Skrinka\config.toml` is created with defaults on the first start. Edit it ("Open config
file" in the tray menu), then choose "Reload config". If the file has an error, a message says
so and the previous settings stay in use.

| Key                 | Default                           | Meaning                                                                 |
|---------------------|-----------------------------------|-------------------------------------------------------------------------|
| `save_dir`          | `<Pictures>\Screenshots`          | Folder for auto-saved screenshots (created when needed)                 |
| `filename_format`   | `Screenshot %Y-%m-%d %H-%M-%S`    | strftime template, same syntax as ScreenCloud; `.png` is added; an invalid template falls back to the default |
| `hotkey_region`     | `Shift+Alt+2`                     | Modifiers `Shift`, `Alt`, `Ctrl`, `Win` plus one key, joined with `+`     |
| `hotkey_fullscreen` | `Shift+Alt+1`                     | Same format                                                             |
| `fullscreen_mode`   | `cursor_monitor`                  | `cursor_monitor` (monitor under the mouse) or `all_monitors`            |
| `last_save_as_dir`  | empty                             | Maintained by the program                                               |

Windows paths can be written as `'C:\Users\me\Pictures'` (single quotes, backslashes as they are)
or `"C:\\Users\\me\\Pictures"`. Characters Windows does not allow in file names (`\ / : * ? " < >
|`) are replaced with `-`; if a file with the same name exists, ` (2)`, ` (3)`, … is added.

The log is `%APPDATA%\Skrinka\skrinka.log`; it is cleared at startup once it grows past 2 MB.

## Known limitations

- A selection cannot span two monitors (same as ScreenCloud). Each monitor has its own overlay.
- The selection cannot be moved or resized after it is drawn; drag a new one instead.
- No "capture active window", no editor (arrows, text, blur), no uploads, no notifications, PNG only.
- `Alt+Shift` is also the Windows shortcut for switching keyboard layouts (ScreenCloud had the
  same default hotkeys). If it gets in the way, choose other hotkeys in the config.

## How it works (for maintainers)

- One process, one main thread running the `winit` event loop; the tray icon (`tray-icon`),
  global hotkeys (`global-hotkey`) and overlay windows live there. PNG encoding, the clipboard,
  the Save-as dialog and error message boxes run on short-lived worker threads.
- Capture is one GDI `BitBlt` of the whole virtual screen. The process is per-monitor DPI aware,
  so capture, monitor and window coordinates are all physical pixels; nothing is scaled.
- The overlay windows are created hidden and get their exact physical position and size again
  after creation (winit would otherwise rescale them when they land on a monitor with a different
  scale factor). The one under the cursor is shown last so it gets the keyboard focus; winit's
  `focus_window()` is avoided because it injects an Alt press that, with Shift still held, can
  switch the keyboard layout.
- The overlay font (Segoe UI, else Arial) is parsed per overlay on a helper thread and dropped
  afterwards; keeping it loaded would cost about 20 MB of idle memory. The parse leaves about
  5 MB of freed heap committed, so the app returns it to Windows (`HeapOptimizeResources`) when
  an overlay is cancelled and when a worker finishes.
- The overlay windows have drag and drop turned off: otherwise winit calls `OleInitialize` for
  every window and never undoes it, which keeps OLE's DLLs and threads loaded.
- Idle memory that grows after use and is owned by Windows, not by Skrinka:
  - The Save-as dialog loads about 90 shell DLLs (network, OneDrive and other namespace
    extensions) that stay loaded: about +19 MB working set, +13 MB private.
  - The clipboard keeps the last copied image in the copying process until the next copy, so
    8 MB after a full-screen capture of a 1920×1080 monitor.
  - Reading the screen maps a copy of the whole virtual screen (4 bytes per pixel) into the
    process. It is shared memory that Windows releases on its own schedule; it counts in the
    working set but not in private bytes.
- `global-hotkey` and `muda` accept their event handler only once per process, so "Reload config"
  re-registers the hotkeys and updates shared ids instead of replacing the handler.

## Manual test checklist

- [ ] Start → tray icon appears; starting a second copy does nothing.
- [ ] `Shift+Alt+1` with the cursor on monitor 1 → PNG of monitor 1 only; on monitor 2 → monitor 2 only. Pasting into Paint works.
- [ ] `Shift+Alt+2` → both monitors dim; help text visible; crosshair cursor.
- [ ] Drag on monitor 2 → region is bright, red border, size label updates while dragging.
- [ ] `Enter` → overlay closes, file in `save_dir`, clipboard has the image; the file shows exactly the selected area (check a known UI element at the edges).
- [ ] `Esc` and right click cancel; no file is written.
- [ ] `Ctrl+S` → Save dialog opens in `save_dir` the first time with the generated name; choose another folder → the next `Ctrl+S` opens there.
- [ ] `Ctrl+C` → clipboard only, no file written.
- [ ] With the Ukrainian keyboard layout active: `Ctrl+C`, `Ctrl+S`, `Enter`, `Esc` still work in the overlay.
- [ ] Set `filename_format` to something invalid (`%Q%`) → Reload config → capture still works with the default name, warning in the log.
- [ ] "Run on startup" toggles the registry value (`regedit`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, or Task Manager → Startup).
- [ ] Idle memory stays small (< 20 MB): about 8 MB working set at start, about 12 MB (3 MB private) after region captures. A full-screen capture or the Save-as dialog adds memory owned by Windows (see "How it works"). The exe is a few MB.
