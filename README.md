# Skrinka

A small screenshot tool for Windows 10/11, written in Rust. It replaces ScreenCloud for one job:
press a hotkey, drag a rectangle (or take the whole screen or the active window), and get the
image on the clipboard and/or as a PNG file.

It lives in the system tray, has no main window, and never downloads anything.

## Download

Get `skrinka.exe` (or the zip with the README, license and changelog) from
[Releases](https://github.com/cropsp/skrinka/releases). It is a single file of about 1.7 MB;
put it in any folder and run it. Each release has a `SHA256SUMS.txt`; compare it with
`Get-FileHash skrinka.exe` in PowerShell.

The exe is not code-signed, so Windows SmartScreen may say "Windows protected your PC" the first
time. Click **More info** → **Run anyway**. Or build it yourself (see below):

```
cargo install --git https://github.com/cropsp/skrinka --locked
```

Close ScreenCloud before starting Skrinka: both use `Shift+Alt+1` / `Shift+Alt+2` / `Shift+Alt+3`
by default, and whoever starts first gets the hotkeys. Skrinka shows a warning if it cannot
register a hotkey.

## Building

Requirements: Rust (stable, `x86_64-pc-windows-msvc`) and the Visual Studio C++ Build Tools with
a Windows SDK (its `rc.exe` compiles the exe's icon and version information).

```
cargo build --release
```

The program is `target\release\skrinka.exe`. If you move it after enabling "Run on startup",
untick that option in Settings and save, then tick it and save again, so the registry points at
the new location.

Run the unit tests with `cargo test`. The build number in Settings and in the log is the number
of commits, so build from a git clone (`cargo install --git` is fine) to get a meaningful one.

## Using it

| Hotkey        | Action                                                                  |
|---------------|-------------------------------------------------------------------------|
| `Shift+Alt+1` | Capture the monitor under the mouse cursor → clipboard + PNG file        |
| `Shift+Alt+2` | Region capture: the screens freeze and dim, drag a rectangle with the mouse |
| `Shift+Alt+3` | Capture the active window → clipboard + PNG file                        |

In the region overlay:

| Key / mouse                   | Action                                                    |
|-------------------------------|-----------------------------------------------------------|
| Drag outside the selection    | Draw a new selection (a plain click clears it)            |
| Drag inside the selection     | Move it (it stays on its monitor)                         |
| Drag an edge or a corner      | Resize it (dragging past the opposite side flips it)      |
| Arrow keys / `Shift`+arrows   | Move the selection by 1 px / 10 px                        |
| `Enter`                       | Copy to clipboard and save a PNG in `save_dir`            |
| `Ctrl+C`                      | Copy to clipboard only                                    |
| `Ctrl+S`                      | "Save as" dialog (opens in the last "Save as" folder) + clipboard |
| `Esc` / right click           | Cancel                                                    |

The cursor shows what a press would do (move, resize, draw), and the selection has handles on its
corners and edge midpoints. Keys work with any keyboard layout, including Ukrainian.

Window capture takes the window as you see it, title bar included (`window_capture = 'client'`
leaves the frame and title bar out). A maximized window is cut to its monitor. It is a piece of
the screen, like in ScreenCloud: another window lying over the active one is in the picture. With
the desktop, the taskbar or a Skrinka dialog in front, it captures the monitor under the cursor
instead, and the log says why.

After a screenshot is saved, a notification shows the file name and folder (see `notifications`).

Tray menu: Capture region, Capture full screen, Capture window (each with its hotkey), Open
screenshots folder, Settings…, Quit. Double-clicking the tray icon opens the screenshots folder.
Captures started from the menu wait 300 ms so the menu is not in the picture. Opening the menu
makes Skrinka's hidden tray window the active one, so "Capture window" from the menu takes the
topmost window of another program instead, normally the one you were working in before.

## Settings

"Settings…" in the tray menu opens a window on the monitor under the mouse. At the top it shows
the version and the build: `Skrinka 0.3.0` and `Build 72 · a1b2c3d · 2026-10-20` (the number of
commits, the commit, the build date; `-dirty` after the commit means uncommitted changes). The
first line of the log has the same.

- **Hotkeys:** click **Change** and press the new combination, for example `Ctrl+Shift+F9`. A key
  needs at least one modifier (`Ctrl`, `Shift`, `Alt`, `Win`), except `F1`–`F24` and
  `Print Screen`. `Esc` keeps the old hotkey, `Backspace` disables it. While the window is open
  the hotkeys are paused, so pressing them records them instead of taking a screenshot.
- **Saving:** the folder (**Browse…** to pick one) and the file name template.
- **Capture:** what full-screen and window captures take, and when a notification appears.
- **Run on startup.**

**Save** (or `Enter`) checks the values first: an empty folder, a template with an unknown `%`
code or two identical hotkeys are refused with a message. Saved settings apply at once, and a
notification says so. **Cancel** or `Esc` closes the window without changes. The GitHub link
opens the project page.

## Configuration file

Settings are stored in `%APPDATA%\Skrinka\config.toml`, created with defaults on the first start.
It can still be edited by hand; edits apply at the next start, or when Settings is opened (it
shows what is in the file) and saved. If the file has an error at startup, a message says so.

| Key                 | Default                           | Meaning                                                                 |
|---------------------|-----------------------------------|-------------------------------------------------------------------------|
| `save_dir`          | `<Pictures>\Screenshots`          | Folder for auto-saved screenshots (created when needed)                 |
| `filename_format`   | `Screenshot %Y-%m-%d %H-%M-%S`    | strftime template, same syntax as ScreenCloud; `.png` is added; an invalid template falls back to the default |
| `hotkey_region`     | `Shift+Alt+2`                     | Modifiers `Ctrl`, `Shift`, `Alt`, `Super` (the Windows key) plus one key, joined with `+`; `''` disables it |
| `hotkey_fullscreen` | `Shift+Alt+1`                     | Same format                                                             |
| `hotkey_window`     | `Shift+Alt+3`                     | Same format                                                             |
| `fullscreen_mode`   | `cursor_monitor`                  | `cursor_monitor` (monitor under the mouse) or `all_monitors`            |
| `window_capture`    | `visible`                         | `visible` (frame and title bar, as you see the window) or `client` (contents only) |
| `notifications`     | `saves`                           | `saves` (after a PNG is saved), `all` (also after `Ctrl+C`) or `off`     |
| `last_save_as_dir`  | empty                             | Maintained by the program                                               |

Windows paths can be written as `'C:\Users\me\Pictures'` (single quotes, backslashes as they are)
or `"C:\\Users\\me\\Pictures"`. Characters Windows does not allow in file names (`\ / : * ? " < >
|`) are replaced with `-`; if a file with the same name exists, ` (2)`, ` (3)`, … is added.

The log is `%APPDATA%\Skrinka\skrinka.log`; it is cleared at startup once it grows past 2 MB.

## Known limitations

- A selection cannot span two monitors (same as ScreenCloud). Each monitor has its own overlay.
- Window capture is screen-based: whatever overlaps the active window is in the picture.
- Notifications appear only if Windows allows notifications for `skrinka.exe` (Settings → System
  → Notifications) and Focus Assist / Do Not Disturb is off. Clicking one does nothing. They also
  stay in the notification centre.
- No editor (arrows, text, blur), no uploads, PNG only.
- `Alt+Shift` is also the Windows shortcut for switching keyboard layouts (ScreenCloud had the
  same default hotkeys). If it gets in the way, choose other hotkeys in Settings.
- The Settings window is in English only.

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
- `global-hotkey` and `muda` accept their event handler only once per process, so applying new
  settings re-registers the hotkeys and updates shared ids instead of replacing the handler.
- The Settings window is a native dialog built from an in-memory `DLGTEMPLATEEX` and run with
  `DialogBoxIndirectParamW` on its own thread, without an owner (an owner on the event-loop
  thread would tie the two threads' input together). Dialogs from templates scale per monitor by
  themselves in a per-monitor-v2 process; only the big title font and the icons are redone on
  `WM_DPICHANGED`. The global hotkeys are unregistered while it is open so the hotkey fields can
  record them. It costs about 0.7 MB private memory the first time (common controls v6), and
  nothing more on later opens: its thread ends with the window.
- `build.rs` puts the build number (commit count), commit and build date into the binary and
  embeds the icon, the version information and the common-controls v6 manifest. Without git
  the build line says `Build 0 · unknown`.
- All user-visible text is in `src/strings.rs`, ready for a translation.
- Region overlay: a press is hit-tested against the selection (8 px around its edges): outside
  draws a new one, inside moves it, an edge or corner resizes it around the opposite, fixed
  corner. The hit test, the drag math and the arrow-key nudge are pure functions with unit tests.
- Window capture reads the foreground window's DWM extended frame bounds (the window as drawn,
  without the invisible resize borders of Windows 10/11), or its client rectangle. A maximized
  window's frame reaches past its monitor by the border width, so it is cut to the monitor under
  its centre. The pixels come from the same virtual-screen capture as everything else;
  `PrintWindow` is not used (unreliable for GPU-rendered apps).
- Notifications are tray balloons (`Shell_NotifyIconW` with `NIF_INFO`) on the icon `tray-icon`
  created; Windows 10/11 show them as toasts, with no app registration. `tray-icon` does not
  expose the icon's `uID`: in 0.26.1 it is 2, because `TrayIconBuilder::new()` takes 1 from the
  same counter for the icon's id. The startup log line `tray icon id verified` confirms it via
  `Shell_NotifyIconGetRect`; if that fails, notifications stay off for the session.

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
- [ ] Set `filename_format` to something invalid (`%Q%`) in `config.toml` by hand → restart → capture still works with the default name, warning in the log.
- [ ] "Run on startup" in Settings toggles the registry value (`regedit`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, or Task Manager → Startup).
- [ ] The first log line is `Skrinka 0.3.0 (Build N · sha · date) starting`; the tray tooltip says `Skrinka 0.3.0`; Explorer shows the Skrinka icon and "Skrinka screenshot tool" under Properties → Details.
- [ ] "Settings…" opens the window centred on the monitor under the mouse, with the version and build line at the top; on the other monitor it looks the same (same size, sharp text).
- [ ] `Tab` cycles through the controls, `Enter` saves, `Esc` cancels; the GitHub link opens the browser.
- [ ] Change → press `Ctrl+Shift+F9` → the field shows `Ctrl+Shift+F9`; Save → "Settings saved and applied." notification, the hotkey works at once, `config.toml` and the log show it.
- [ ] While recording: a bare letter is refused (beep), `Esc` restores the old hotkey, `Backspace` clears it; after Save the menu shows "disabled" next to that capture.
- [ ] While Settings is open `Shift+Alt+2` does nothing; after Cancel it works again.
- [ ] Browse… picks a folder (the Settings window is disabled meanwhile); a template like `%Q%` is refused with a message.
- [ ] A second "Settings…" brings the open window to the front; Quit with the window open closes both.
- [ ] Private memory stays the same after opening and closing Settings several times.
- [ ] Idle memory stays small (< 20 MB): about 8 MB working set at start, about 12 MB (3 MB private) after region captures. A full-screen capture or the Save-as dialog adds memory owned by Windows (see "How it works"). Compare private bytes: the working set can also hold Windows' shared copy of the virtual screen (4 bytes per pixel, about 16 MB for two 1920×1080 monitors) until Windows releases it. The exe is a few MB.
- [ ] Draw a selection, hover inside → move cursor; drag → it follows and cannot leave the monitor.
- [ ] Hover an edge → resize cursor; drag the right edge left past the left edge → the rectangle flips.
- [ ] Drag a corner; the size label updates; the handles stay on the corners and edge midpoints.
- [ ] Arrow keys move the selection by 1 px, `Shift`+arrows by 10 px; it stops at the monitor edge.
- [ ] Click outside the selection without moving → the selection disappears; the help text does not come back.
- [ ] `Enter` after moving/resizing saves exactly the shown rectangle (check the edges).
- [ ] `Shift+Alt+3` on a normal window → PNG of that window with its title bar, no desktop around it; on a maximized window → the whole monitor minus the taskbar, no black or other-monitor strip at the edges.
- [ ] Window captures = "Window contents only" in Settings → the PNG has no title bar.
- [ ] `Shift+Alt+3` with the desktop or the taskbar active → still a capture (the monitor under the cursor), and the log says why.
- [ ] "Capture window" from the tray menu → the window you were working in.
- [ ] After `Enter`, `Shift+Alt+1` and `Shift+Alt+3` a "Screenshot saved" notification shows the file name; Notifications = "Off" in Settings → none; "After every capture" → also one after `Ctrl+C`.
- [ ] Ukrainian layout: arrows, `Ctrl+C`, `Ctrl+S`, `Enter`, `Esc` still work in the overlay.

## License

MIT, see [LICENSE](LICENSE). It covers every version of Skrinka, including v0.1.0 and v0.2.0.
The licenses of the crates compiled into `skrinka.exe` are in
[THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html).

Skrinka was inspired by ScreenCloud and uses the same default hotkeys, but contains no code,
text or images from it.
