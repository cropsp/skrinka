# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Skrinka is a Windows-only (10/11, x86_64, MSVC) tray screenshot tool in Rust that replaces ScreenCloud. `TASK.md` is the full spec: scope, pinned crate versions, behaviour, DPI rules, pitfalls and acceptance tests. Read it before changing behaviour. Its "out of scope" list (selection handles, window capture, editor, uploads, toasts, JPG, settings GUI, CLI args) applies; **never add auto-update or anything that downloads**. `README.md` holds the user docs and the manual test checklist.

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
- **Keys:** compare `physical_key` (`KeyCode::KeyC`, …), never `logical_key`. The user switches between English and Ukrainian layouts.
- **Finishing:** on finish, `App` drops the overlay *before* spawning the output worker, so a dialog never opens underneath it. While a Save-as dialog is open (`save_as_open`), the region hotkey is ignored.

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

## Verifying behaviour

Unit tests cover the pure logic: template, unique names, crop math, rects, config round-trip, pixel conversion, labels.

End-to-end behaviour can be checked without a person:
- Inject Shift+Alt+1/2, mouse drags and keys with `SendInput`/`keybd_event`. `RegisterHotKey` responds to injected input.
- Send real scancodes (`MapVirtualKey`) so winit computes `physical_key` correctly.
- Confirm results with the log lines, the new PNGs in `save_dir`, and a pixel comparison of a region PNG against a full-screen PNG at the logged rect.
- Always check that an overlay or dialog is the foreground window before injecting keys.
