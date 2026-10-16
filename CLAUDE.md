# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Skrinka is a Windows-only (10/11, x86_64, MSVC) tray screenshot tool in Rust that replaces ScreenCloud. `TASK.md` is the v1 spec (scope, pinned crate versions, behaviour, DPI rules, pitfalls, acceptance tests) and `TASK-v2.md` the v2 spec, which adds selection handles, active-window capture and notifications after a save. Read both before changing behaviour. Still out of scope: an editor (arrows, text, blur), uploads, JPG, a settings GUI, CLI args, a selection spanning two monitors, click-to-open on notifications; **never add auto-update or anything that downloads**. `README.md` holds the user docs and the manual test checklist.

## Commands

`cargo` lives in `%USERPROFILE%\.cargo\bin`, which may not be on PATH in a fresh shell (`$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path`).

```
cargo build --release                                  # target\release\skrinka.exe, no console
cargo build                                            # debug: console + "Debug: dump capture" tray item
cargo test                                             # unit tests (all live in #[cfg(test)] modules in src/)
cargo test output::tests::default_template             # one test; any substring filter works, e.g. `cargo test crop`
cargo clippy --release --all-targets -- -D warnings    # must stay clean, including test code
```

- **Running exe:** stop a running `skrinka.exe` before rebuilding; the running exe locks the file.
- **Single instance:** a second launch exits silently (mutex `Local\Skrinka.SingleInstance`).
- **Ghost tray icons:** force-killing the process leaves a ghost tray icon until the mouse passes over it.
- **Hotkey conflict:** ScreenCloud uses the same default hotkeys. If it's running, registration fails and Skrinka shows a warning.
- **Runtime files** are in `%APPDATA%\Skrinka\`: `config.toml` and `skrinka.log`. Every capture, save, reload and toggle writes a log line, so the log is the way to confirm an action really happened.
- **Logging rule:** keep it that way when adding features.

## Architecture

**One main thread runs the `winit` 0.30 event loop** (`App: ApplicationHandler<UserEvent>` in `app.rs`). The tray icon, global hotkeys, overlay windows and config changes all live on it.
- Slow or blocking work runs on short-lived `std::thread`s and reports back through `UserEvent::WorkerDone`. That covers PNG encoding, the clipboard, the Save-as dialog, error dialogs and the 300 ms tray-menu delay.
- Workers never touch `App`.
- Tray, hotkeys and windows are created in `resumed()`, guarded by `initialized`.

**Event plumbing (`app.rs`, `hotkeys.rs`):**
- The `global-hotkey`, `muda` and `tray-icon` handlers must be `Send + Sync`. `EventLoopProxy` isn't `Sync` on Windows, so it's shared as `Arc<Mutex<EventLoopProxy>>` (`SharedProxy`, sent through `send_event`).
- All three crates store their handler in a `OnceCell`/`OnceLock`. A second `set_event_handler` call is silently ignored.
- So "Reload config" doesn't reinstall the hotkey handler. It re-registers the hotkeys and updates the ids in `Arc<Mutex<Ids>>`, which the handler reads.
- Hotkey events also fire on release; only `HotKeyState::Pressed` counts.

**No `rfd` dialog on the event-loop thread.**
- From callbacks, use `dialog::error_async` or `dialog::warning_async`.
- Blocking `dialog::error` is only for worker threads, and for the fatal error in `main` after the loop has ended.

**Coordinates (`capture.rs`, `util.rs`):**
- The process is per-monitor DPI aware (winit does this). So the GDI `BitBlt` capture, `GetCursorPos`, winit monitor rects and `CursorMoved` are all physical pixels.
- Never use `scale_factor()` or logical sizes.
- The virtual-screen origin can be negative. `Capture::screen_to_capture` converts monitor/screen rects into capture coordinates.
- `Rect` sizes are pixel counts. `Rect::from_corners` includes both corners.
- Capture pixels are `0x00RRGGBB`, which softbuffer uses as is. They are converted to RGBA only in `util::crop_rgba` for saving and the clipboard.

**Region overlay (`overlay.rs`):**
- The screen is captured *before* any window exists.
- There is one borderless, always-on-top window per monitor, and a selection lives on a single monitor. Selection state is session-wide; each window keeps a `base` and a `dimmed` frame buffer.
- **winit 0.30 quirks:**
  - It creates windows at the default position and moves them afterwards. Landing on a monitor with a different scale factor triggers a buffered `ScaleFactorChanged` and a rescale.
  - So windows are created hidden, the exact physical position and size are applied again after creation, and `ScaleFactorChanged` writes the monitor size back through `inner_size_writer`.
  - Stale `Resized` events from creation arrive late. Judge the size by `inner_size()`, not by the event.
- **Focus:** don't call winit's `focus_window()`. It injects an Alt press, which together with the held Shift of Shift+Alt+2 can switch the keyboard layout. Instead, the window under the cursor is created `with_active(true)` and shown last.
- **Keys:** compare `physical_key` (`KeyCode::KeyC`, …), never `logical_key`. The user switches between English and Ukrainian layouts. Arrow keys act on repeats; Enter/C/S/Esc ignore them.
- **Selection model:** a left press is hit-tested against the selection on the same window (`hit_test`, `HANDLE_HIT` = 8 px around the edges; corners beat edges, left beats right, top beats bottom). `press_kind` turns the hit into `DragKind::Draw` (outside: clears the selection), `Move { grab }` (inside) or `Resize { fixed, moving, .. }` (edge/corner; `fixed` is the opposite corner and never changes, so crossing it flips the rect). `Drag::rect` is the live rect; on release it becomes the selection, except a `Draw` click without movement, which leaves none. These and `nudge` are pure and unit-tested. The cursor (`update_cursor`) follows the hit only while no drag is in progress.
- **Finishing:** on finish, `App` drops the overlay *before* spawning the output worker, so a dialog never opens underneath it. While a Save-as dialog is open (`save_as_open`), the region hotkey is ignored.

**Window capture (`window.rs`, `App::capture_window`):**
- The screen is captured first, then the foreground window's rect is read: DWM `EXTENDED_FRAME_BOUNDS` for `visible` (no invisible resize borders), `GetClientRect` + `ClientToScreen` for `client`. Physical pixels, like everything else.
- A maximized window's frame sticks out of its monitor by the border width, so `target_rect` cuts it to the monitor under its centre before converting to capture coordinates.
- Minimized, Skrinka's own, shell windows (`Progman`, `WorkerW`, `Shell_TrayWnd`, `Shell_SecondaryTrayWnd`) or an off-screen rect → a logged warning and the cursor-monitor capture instead.
- From the tray menu the foreground window is tray-icon's hidden `tray_icon_app` window (it calls `SetForegroundWindow` before `TrackPopupMenu`, and Windows does not give the focus back), so `from_menu` walks the Z order for the topmost visible, uncloaked, activatable window of another process.

**Text (`text.rs`):**
- fontdue parses every glyph of Segoe UI (≈20 MB, ≈60 ms).
- So the font is loaded per overlay, on a thread started together with the capture, and dropped with the overlay. Keeping it loaded breaks the < 20 MB idle-memory target.

**Output (`output.rs`):**
- Clipboard first, then the file.
- `format_filename` validates the strftime template with `StrftimeItems::parse()` before formatting, because chrono panics on bad templates.
- Files are created with `create_new` in a ` (2)`, ` (3)` loop, so two captures in the same second can't overwrite each other.

**Config (`config.rs`, `App::remember_save_as_dir`):**
- `#[serde(default)]` gives defaults for missing keys.
- The file is written only when it is created, and when `last_save_as_dir` changes. Then it is re-read from disk first, so unreloaded user edits survive; if it doesn't parse, it is left alone.

**Tray (`tray.rs`):**
- The icon is generated in code.
- Hotkey text is a `\t`-separated part of the label, not a muda accelerator.
- muda toggles `CheckMenuItem` itself before the event arrives. The autostart handler reads the new state and reverts it if the registry write fails (`autostart.rs`, `HKCU\…\Run\Skrinka`).
- **Notifications:** `Tray::notify` sends `Shell_NotifyIconW(NIM_MODIFY)` with `NIF_INFO` to tray-icon's icon (`hWnd` = `TrayIcon::window_handle()`), which Windows shows as a toast. Main thread only; called from `App::notify_done` after a worker without errors.
- **Tray uID = 2:** tray-icon does not expose the icon's `uID`. In 0.26.1 it comes from a process-wide counter starting at 1, and `TrayIconBuilder::new()` draws 1 for the `TrayIconId`, so the icon gets 2 (`TRAY_ICON_UID`). `Tray::verify_icon_id` checks it at startup with `Shell_NotifyIconGetRect` against `TrayIcon::rect()` and logs `tray icon id verified`. If it fails, `notifications_ok` stays false for the session. An icon hidden in the overflow flyout answers `S_FALSE` (no position); windows-rs treats that as `Ok`, tray-icon's `rect()` as `None`, and that pair also counts as verified. Re-check the uID when bumping tray-icon.

## Verifying behaviour

Unit tests cover the pure logic: template, unique names, crop math, rects, config round-trip, pixel conversion, labels, hit test, drag/resize/move/nudge math, handles, window target rects, UTF-16 truncation.

End-to-end behaviour can be checked without a person:
- Inject Shift+Alt+1/2/3, mouse drags and keys with `SendInput`/`keybd_event`. `RegisterHotKey` responds to injected input.
- Send real scancodes (`MapVirtualKey`) so winit computes `physical_key` correctly; arrow keys need `KEYEVENTF_EXTENDEDKEY`, or winit sees numpad keys.
- Confirm results with the log lines, the new PNGs in `save_dir`, and a pixel comparison of a region PNG against a full-screen PNG at the logged rect (`fullscreen_mode = 'all_monitors'` makes capture and PNG coordinates the same). A window capture of Notepad differs only by its blinking caret.
- Always check that an overlay or dialog is the foreground window before injecting keys. Windows' foreground lock can keep a fresh overlay from getting the focus; the first mouse press on it fixes that.
- Cursor shapes: compare `GetCursorInfo().hCursor` with `LoadCursor(NULL, IDC_SIZEALL/IDC_SIZEWE/…)`.
- The tray menu: find the icon with `Shell_NotifyIconGetRect(tray_icon_app window, uID 2)`. If the icon is in the overflow flyout, click the taskbar's chevron first. Navigate the menu with Down/Enter.
