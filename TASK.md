# TASK: "Skrinka" — a lightweight screenshot tool for Windows, written in Rust

> Working name is **Skrinka** (Ukrainian "скринька" = a small box or chest — the place where screenshots go). Use it everywhere exactly as written here: binary name `skrinka`, config dir `Skrinka`, mutex name, registry value, tray tooltip.

## 0. Context and goal

The user currently uses **ScreenCloud 1.5.3** (C++/Qt, with embedded Python) only for one thing: press a global hotkey, drag a rectangle on the screen with the mouse, and get the image into the clipboard and/or a PNG file. ScreenCloud crashes randomly and drags along Qt + Python + OpenSSL for a trivial job. We are replacing it with a small, dependency-light Rust program that does exactly that job and nothing more.

**Target platform: Windows 10/11 only, x86_64, MSVC toolchain.** Do not spend effort on Linux/macOS code paths. `cfg(windows)` everywhere is fine.

**User's setup:** two monitors (DPI scaling unknown — the code must handle mixed/non-100% scaling correctly, see §6). Keyboard layouts: English and Ukrainian (see §7.3 — this matters for key handling).

## 1. Scope

### 1.1 In scope (v1 — this task)

1. Runs in the system tray, no main window. No console window in release builds.
2. Two global hotkeys (defaults identical to ScreenCloud):
   - `Shift+Alt+1` — capture full screen (the monitor under the mouse cursor).
   - `Shift+Alt+2` — region capture: frozen-screen overlay, drag a rectangle with the mouse.
3. In the region overlay:
   - `Enter` → copy to clipboard **and** auto-save PNG into the default folder.
   - `Ctrl+C` → copy to clipboard only.
   - `Ctrl+S` → native "Save as" dialog (pre-filled with the generated name, opened in the last "Save as" folder) + copy to clipboard.
   - `Esc` or right mouse button → cancel.
4. Full-screen hotkey → clipboard + auto-save, no overlay.
5. File names generated from a strftime-style template (same syntax ScreenCloud uses, e.g. `Screenshot %Y-%m-%d %H-%M-%S`).
6. Tray menu: Capture region, Capture full screen, Open screenshots folder, Open config file, Reload config, Run on startup (checkbox), Quit.
7. Config in a TOML file. Log file. Single-instance guard. Autostart via registry.

### 1.2 Explicitly OUT of scope for v1 (do NOT implement now, keep the code open for it)

- Moving/resizing the selection after it is drawn (handles) — v2.
- "Capture active window" hotkey — v2.
- Screenshot editor (arrows, text, blur) — v3.
- Uploads (FTP/Imgur/etc.), notifications/toasts, JPG output, sounds, settings GUI, i18n, CLI arguments, auto-update. **No auto-update ever** — the user explicitly does not want the program to download anything.

If you find yourself adding anything from this list, stop.

## 2. Tech stack (versions verified against crates.io on 2026-10-16 — pin exactly these)

```toml
[package]
name = "skrinka"
version = "0.1.0"
edition = "2021"

[dependencies]
winit        = "0.30"          # resolves to 0.30.13. Do NOT use 0.31 (it is a beta with a different API).
softbuffer   = "0.4"           # 0.4.8 — software framebuffer for the overlay
global-hotkey = "0.8"          # 0.8.0 — tauri-apps, RegisterHotKey wrapper
tray-icon    = "0.26"          # 0.26.1 — tauri-apps; menus come from its re-exported `muda` (0.21)
arboard      = "3"             # 3.7.0 — clipboard (feature "image-data" is on by default; keep it)
image        = { version = "0.25", default-features = false, features = ["png"] }   # 0.25.10
rfd          = "0.17"          # 0.17.2 — native Save-as / message dialogs (IFileDialog on Windows, no GTK)
toml         = "1"             # 1.1.x
serde        = { version = "1", features = ["derive"] }
chrono       = "0.4"           # 0.4.45 — strftime formatting
dirs         = "7"             # 7.0.0 — config_dir(), picture_dir()
fontdue      = "0.9"           # 0.9.4 — rasterize the size label / help text in the overlay
log          = "0.4"
simplelog    = "0.12"
anyhow       = "1"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_Threading",
    "Win32_Security",            # needed because CreateMutexW's signature mentions SECURITY_ATTRIBUTES
] }
winreg = "0.56"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

Why these and not alternatives:
- **No Qt/GTK/egui/iced/Tauri/webview.** The overlay is just pixels in a borderless window; `winit + softbuffer` is enough and keeps the exe at a few MB.
- **No `xcap` or other capture crate.** We capture with 10 lines of GDI (`BitBlt`) — see §6.2. This guarantees that capture coordinates and `winit` window coordinates live in the same physical-pixel space.
- `global-hotkey` and `tray-icon` are designed to run inside a `winit` event loop (they create hidden Win32 windows on the main thread and rely on that thread's message pump).

## 3. Architecture

Single process, **one main thread running the `winit` event loop** (`EventLoop::<UserEvent>::with_user_event().build()` → `run_app(&mut app)`), plus short-lived worker threads for file writing / clipboard / the Save-as dialog, so the event loop never blocks.

```
src/
  main.rs        — entry: single-instance guard, logging, config, build event loop, run App
  app.rs         — `App: ApplicationHandler<UserEvent>`; owns tray, hotkeys, config, optional OverlaySession
  config.rs      — Config struct, load/save, defaults, path
  capture.rs     — GDI virtual-screen capture → `Capture { x0, y0, width, height, pixels: Vec<u32> }`  (0x00RRGGBB)
  overlay.rs     — OverlaySession: one borderless window per monitor, selection state, rendering
  text.rs        — fontdue helpers: load Segoe UI from C:\Windows\Fonts, draw a string into a u32 buffer
  output.rs      — filename template, PNG save with collision suffix, clipboard set, Save-as flow (runs on worker thread)
  tray.rs        — tray icon + menu construction, generated 32×32 icon
  autostart.rs   — HKCU Run key read/write
  util.rs        — Rect type, normalize, clamp, crop
```

### 3.1 UserEvent (sent through `EventLoopProxy<UserEvent>`)

```rust
enum UserEvent {
    HotkeyRegion,            // from global-hotkey handler (Pressed only!)
    HotkeyFullscreen,
    TrayMenu(MenuId),        // from muda MenuEvent handler
    TrayDoubleClick,         // from TrayIconEvent handler
    DelayedCapture(CaptureKind),  // sent by a sleeping thread 300 ms after a tray-menu capture click
    WorkerDone(WorkerResult),     // { kind: Clipboard|AutoSave|SaveAs, saved_path: Option<PathBuf>, error: Option<String> }
}
```

Wire the three tauri-apps event sources to the proxy once, in `resumed()` (they must be set after the event loop exists).

**Compile-time trap:** all three `set_event_handler` functions require `F: Fn(..) + Send + Sync + 'static`, but winit's `EventLoopProxy<T>` is `Send` and **not `Sync`** on Windows (verified in `winit-0.30.13/src/platform_impl/windows/event_loop.rs`). A closure that captures the proxy directly will not compile. Wrap it: `let proxy = Arc<Mutex<EventLoopProxy<UserEvent>>>` and `proxy.lock().unwrap().send_event(..)` inside the closures.

```rust
use std::sync::{Arc, Mutex};
let shared = Arc::new(Mutex::new(event_loop_proxy));      // created once in main from `event_loop.create_proxy()`

let p = shared.clone();
let (id_region, id_full) = (self.hk_region.id(), self.hk_full.id());
GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
    if e.state != HotKeyState::Pressed { return; }
    let ev = if e.id == id_region { UserEvent::HotkeyRegion }
             else if e.id == id_full { UserEvent::HotkeyFullscreen }
             else { return };
    let _ = p.lock().unwrap().send_event(ev);
}));
let p = shared.clone();
MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
    let _ = p.lock().unwrap().send_event(UserEvent::TrayMenu(e.id));
}));
let p = shared.clone();
TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
    if let TrayIconEvent::DoubleClick { .. } = e { let _ = p.lock().unwrap().send_event(UserEvent::TrayDoubleClick); }
}));
```

Note: the hotkey ids captured by the closure become stale after "Reload config" re-registers hotkeys. Either re-install the hotkey handler after re-registering (simplest: call the same setup function again — `set_event_handler` replaces the previous handler), or keep the ids in an `Arc<Mutex<(u32,u32)>>` shared with the closure.

**Important:** `GlobalHotKeyEvent` fires for both `Pressed` and `Released`. Filter on `Pressed`, otherwise every hotkey triggers twice.

### 3.2 Where things are created

- `GlobalHotKeyManager::new()`, `TrayIconBuilder::build()` and all windows: **inside `resumed()` / event callbacks on the main thread**, never before `run_app` and never on another thread. Guard `resumed()` with an `initialized: bool` flag so the tray/hotkeys are created exactly once. Keep the `TrayIcon` and the `GlobalHotKeyManager` alive in `App` for the whole run (dropping the `TrayIcon` removes the icon).
- The winit event loop must run with `ControlFlow::Wait` (default). Do not poll.

## 4. Behaviour specification

### 4.1 Startup (`main.rs`)

1. `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` — release has no console; debug keeps it for printing.
2. Single instance: `CreateMutexW(None, false, w!("Local\\Skrinka.SingleInstance"))`; if `GetLastError() == ERROR_ALREADY_EXISTS` → exit(0) silently. Keep the handle for the process lifetime (just leak it).
3. Logging: file `%APPDATA%\Skrinka\skrinka.log` (`dirs::config_dir().join("Skrinka")`, create dir). If the file is larger than 2 MB at startup, truncate it. `simplelog::WriteLogger::init(LevelFilter::Info, ...)`. Install `std::panic::set_hook` that logs the panic message + location at `error!` level before the default behaviour. Log app version and config path at start.
4. Load config (§4.7). If the file does not exist, write defaults.
5. Build event loop, create `App`, run. Any fatal error before the loop → `rfd::MessageDialog` with the error text, then exit(1).

### 4.2 Tray

- Icon: generate programmatically at startup as 32×32 RGBA (no asset files): a filled rounded square in a mid-grey/blue, with a lighter 12×12 square in the top-left corner (a "frame" glyph). `tray_icon::Icon::from_rgba(vec, 32, 32)`.
- Tooltip: `Skrinka`.
- Menu (muda via `tray_icon::menu::*`), in this order:
  1. `Capture region   Shift+Alt+2` (MenuItem; show the current hotkey text in the label, no muda accelerator)
  2. `Capture full screen   Shift+Alt+1`
  3. separator
  4. `Open screenshots folder`
  5. `Open config file`
  6. `Reload config`
  7. separator
  8. `Run on startup` (CheckMenuItem; initial state = §4.8 `is_enabled()`)
  9. separator
  10. `Quit`
- Keep the `MenuId`s of each item in `App` and dispatch `UserEvent::TrayMenu(id)` by comparing ids.
- Left click and right click both show the menu (default builder behaviour; leave `with_menu_on_left_click` at default). Double-click on the icon → open screenshots folder.
- "Capture region"/"Capture full screen" from the menu: the menu popup needs time to disappear, so spawn a thread that sleeps **300 ms** and then sends `UserEvent::DelayedCapture(kind)`. Hotkeys capture immediately (no delay).
- "Open screenshots folder": `std::process::Command::new("explorer.exe").arg(&cfg.save_dir).spawn()`. Create the folder first if missing.
- "Open config file": `Command::new("notepad.exe").arg(config_path).spawn()`.
- "Reload config": re-read the TOML; on parse error show a `rfd::MessageDialog` (level Error) with the message and keep the old config. On success: `unregister_all` the old hotkeys, register the new ones, update the two capture menu item labels (`MenuItem::set_text`).
- "Quit": `event_loop.exit()`; App's Drop releases the tray icon.

### 4.3 Hotkeys

- Parse from config strings with `cfg.hotkey_region.parse::<global_hotkey::hotkey::HotKey>()`. The crate's parser splits on `+`, accepts `SHIFT`, `ALT`/`OPTION`, `CTRL`/`CONTROL`, `SUPER`/`WIN`, and `1`…`9` map to `Code::Digit1`… (verified in `hotkey.rs::parse_key`). So `"Shift+Alt+2"` parses as-is.
- If parsing fails → log error, fall back to the default for that hotkey.
- If `manager.register(hk)` fails (another app owns the combination) → log + `rfd::MessageDialog` warning "Could not register hotkey X — it is probably used by another program. Change it in config.toml." and keep running.
- Store `hotkey.id()` for both and match `GlobalHotKeyEvent.id` against them.
- Region hotkey while an overlay is already open → ignore. Full-screen hotkey while an overlay is open → ignore.

### 4.4 Full-screen capture (`Shift+Alt+1`)

1. `capture::capture_virtual_screen()` (§6.2).
2. Determine the target rectangle:
   - `fullscreen_mode = "cursor_monitor"` (default): `GetCursorPos` → find the winit monitor whose rect contains the point (`event_loop.available_monitors()`, `position()`/`size()` are physical pixels); crop the capture to that monitor rect. If no monitor contains the point, use `primary_monitor()`.
   - `fullscreen_mode = "all_monitors"`: use the whole capture.
3. Hand the cropped RGBA image to a worker thread with `Action::ClipboardAndAutoSave` (§4.6).

### 4.5 Region capture (`Shift+Alt+2`) — the overlay

#### 4.5.1 Creation

1. Capture the virtual screen **first** (before any window exists, so the overlay is not in the shot).
2. For **each** monitor from `event_loop.available_monitors()` create one window:
   ```rust
   WindowAttributes::default()
       .with_title("Skrinka overlay")
       .with_decorations(false)
       .with_resizable(false)
       .with_transparent(false)
       .with_position(PhysicalPosition::new(mon.position().x, mon.position().y))
       .with_inner_size(PhysicalSize::new(mon.size().width, mon.size().height))
       .with_window_level(WindowLevel::AlwaysOnTop)
       .with_active(true)
       .with_cursor(CursorIcon::Crosshair)
       // winit::platform::windows::WindowAttributesExtWindows:
       .with_skip_taskbar(true)
       .with_undecorated_shadow(false)
   ```
   Wrap each in `Rc<Window>`; create `softbuffer::Context::new(window.clone())` and `softbuffer::Surface::new(&ctx, window.clone())`, call `surface.resize(NonZeroU32(w), NonZeroU32(h))` once.
3. For each window pre-compute two `Vec<u32>` of `w*h`: `base` = the capture cropped to that monitor's rect (already 0x00RRGGBB, see §6.2), and `dimmed` = every channel multiplied by **0.45** (i.e. `(c * 115) >> 8`).
4. After creating all windows call `focus_window()` on the one under the cursor (`GetCursorPos`), then `request_redraw()` on all.
5. One monitor's overlay window covering it exactly; **a selection cannot span two monitors** in v1 (same limitation as ScreenCloud). Document this in README.

#### 4.5.2 State (lives in `OverlaySession`, not per window)

```rust
struct OverlaySession {
    capture: Arc<Capture>,
    windows: Vec<OverlayWindow>,       // index = monitor index
    drag: Option<Drag>,                 // { win: usize, start: (i32,i32), cur: (i32,i32) }  window-local physical px
    selection: Option<Sel>,             // { win: usize, rect: Rect }  window-local, inclusive pixel rect
    ctrl: bool,                         // from WindowEvent::ModifiersChanged
    started: bool,                      // true after first mouse press → hides help text
}
```

Rect semantics: `Rect { x, y, w, h }` with `w = |x1-x0| + 1`, `h = |y1-y0| + 1` (inclusive of both corners). All cursor positions are clamped to `[0, w-1] × [0, h-1]` of their window before use.

#### 4.5.3 Events (all `WindowEvent`s carry a `WindowId`; map it to the window index first)

`MouseInput` in winit carries **no position**, so every `OverlayWindow` keeps `last_cursor: (i32, i32)` (window-local physical px, clamped), updated on every `CursorMoved`; mouse-button handlers read it. While the left button is held, winit keeps mouse capture, so `CursorMoved` keeps arriving even outside the window with out-of-range coordinates — always clamp.

| Event | Action |
|---|---|
| `CursorMoved { position }` | `win.last_cursor = clamp(position)`; if `drag.is_some() && drag.win == idx` → `drag.cur = win.last_cursor`, `request_redraw()` on that window |
| `MouseInput { state: Pressed, button: Left }` | `selection = None; drag = Some{win: idx, start: win.last_cursor, cur: win.last_cursor}; started = true;` redraw all windows (clears a selection drawn on another monitor) |
| `MouseInput { state: Released, button: Left }` | if dragging on this window → `selection = Some(normalize(drag))`, `drag = None`, redraw |
| `MouseInput { state: Pressed, button: Right }` | cancel (§4.5.5) |
| `ModifiersChanged(m)` | `ctrl = m.state().control_key()` |
| `KeyboardInput { event, .. }` with `event.state == Pressed` and `!event.repeat` | see key table |
| `RedrawRequested` | render that window (§4.5.4) |
| `CloseRequested` | cancel |
| `ScaleFactorChanged { inner_size_writer, .. }` | **ignore the suggestion** (do not resize; the window must stay exactly monitor-sized). Just return. |

Key table — **compare `event.physical_key` (`PhysicalKey::Code(KeyCode::…)`), never `logical_key`** (§7.3):

| Key | Condition | Action |
|---|---|---|
| `KeyCode::Escape` | — | cancel |
| `KeyCode::Enter` or `KeyCode::NumpadEnter` | selection valid (w ≥ 2 && h ≥ 2) | finish with `Action::ClipboardAndAutoSave` |
| `KeyCode::KeyC` | `ctrl` && selection valid | finish with `Action::ClipboardOnly` |
| `KeyCode::KeyS` | `ctrl` && selection valid | finish with `Action::SaveAs` |
| anything else | — | ignore |

A finish key while `drag.is_some()` (mouse still held) → ignore.

#### 4.5.4 Rendering a window (inside `RedrawRequested`, with `surface.buffer_mut()`)

1. `buf.copy_from_slice(&dimmed)`.
2. Live rect = `drag` (if on this window) else `selection` (if on this window) else none. If present:
   - copy the rows of `base` inside the rect into `buf` (undimmed region),
   - draw a **2 px** border just **outside** the rect, colour `(227, 65, 51)` (ScreenCloud red), clipped to the window,
   - draw the label `"{w} × {h}"` (U+00D7) with the font at 16 px, white text on a `(28,28,28)` box with 4 px padding, placed above the rect at `(rect.x, rect.y - box_h - 4)`; if that is `< 0`, place it inside the rect at `(rect.x + 4, rect.y + 4)`; if the label would overflow the right edge, shift it left.
3. If `!started`: draw the help line centred at the top of the window, 40 px from the top, same style box: `Drag to select  ·  Enter: copy + save  ·  Ctrl+S: save as  ·  Ctrl+C: copy  ·  Esc: cancel`.
4. `buf.present()`.

Fonts: `text.rs` tries `C:\Windows\Fonts\segoeui.ttf`, then `arial.ttf`; if neither loads, text drawing becomes a no-op (the tool must still work). Rasterize per glyph with `fontdue::Font::rasterize(ch, px)` and alpha-blend coverage into the u32 buffer. Cache nothing fancy; a dozen glyphs per frame is cheap.

Performance expectation: a 2560×1440 window = 3.7 M pixels; `copy_from_slice` + a region copy is a few ms in release. If debug builds feel slow that is acceptable.

#### 4.5.5 Cancel

Drop all overlay windows (`App.overlay = None`). Nothing else.

#### 4.5.6 Finish

1. Translate the selection to capture coordinates: `cx = win.mon_x + rect.x - capture.x0`, `cy = win.mon_y + rect.y - capture.y0`; crop `capture.pixels` → `image::RgbaImage` (convert 0x00RRGGBB → `[r,g,b,255]`). Assert the crop lies inside the capture; if not, log and clamp.
2. Drop the overlay windows **before** doing anything else (so a Save-as dialog never appears under them).
3. Spawn the worker (§4.6) with the image and the action.

### 4.6 Output worker (`output.rs`, runs on `std::thread::spawn`)

Input: `RgbaImage`, `Action`, a snapshot of `Config` (clone), `EventLoopProxy`.

- `ClipboardOnly`: set clipboard.
- `ClipboardAndAutoSave`: set clipboard; `fs::create_dir_all(save_dir)`; `path = unique_path(save_dir, format_filename(template, now))`; save PNG.
- `SaveAs`: build the file name as above; `rfd::FileDialog::new().set_title("Save screenshot").set_directory(dir).set_file_name(name).add_filter("PNG image", &["png"]).save_file()` where `dir = if cfg.last_save_as_dir is a directory { it } else { cfg.save_dir }`. If the user cancels → do nothing (not even clipboard). If a path is returned: append `.png` if the extension is missing, save PNG, set clipboard. Report the chosen path in `WorkerResult.saved_path` so the main thread can store `last_save_as_dir = path.parent()` in the config and persist it.
- Clipboard: `arboard::Clipboard::new()?.set_image(ImageData { width, height, bytes: Cow::Borrowed(rgba) })`. Create the `Clipboard` inside the worker; drop it right after.
- Any error → `error!` to the log **and** a `rfd::MessageDialog` (level Error, title "Skrinka") with the error text; then send `WorkerDone` with `error: Some(..)`. rfd dialogs are fine from a worker thread on Windows.
- Order inside the worker: clipboard first (fast, user can paste immediately), then the file.

#### 4.6.1 File name template (`format_filename`)

- Syntax: chrono/strftime (`%Y %m %d %H %M %S %y %b %j …`) — identical to what ScreenCloud used (`Screenshot at %H-%M-%S` was its default). Any literal text is allowed.
- Validate first: `chrono::format::strftime::StrftimeItems::new(tpl).parse()` — if `Err`, log a warning and use the default template. (A bad template passed straight to `.format().to_string()` **panics**; this guard is mandatory.)
- Then `Local::now().format_with_items(items).to_string()`.
- Sanitize: replace each of `\ / : * ? " < > |` and control chars with `-`; trim whitespace and dots; if the result is empty → `Screenshot`.
- Append `.png` unless the name already ends with `.png` (case-insensitive).
- `unique_path(dir, name)`: if `dir/name` exists → `name (2).png`, `name (3).png`, … (insert before the extension).
- Unit-test this module (see §9.2).

### 4.7 Config (`config.rs`)

Path: `dirs::config_dir()/Skrinka/config.toml` (→ `C:\Users\<user>\AppData\Roaming\Skrinka\config.toml`).

```toml
# Skrinka configuration. Edit, then use "Reload config" in the tray menu.
save_dir = 'C:\Users\<user>\Pictures\Screenshots'      # default: dirs::picture_dir()/Screenshots
filename_format = 'Screenshot %Y-%m-%d %H-%M-%S'       # strftime; ".png" is added automatically
hotkey_region = 'Shift+Alt+2'
hotkey_fullscreen = 'Shift+Alt+1'
fullscreen_mode = 'cursor_monitor'                     # 'cursor_monitor' | 'all_monitors'
last_save_as_dir = ''                                  # maintained by the program
```

- `#[derive(Serialize, Deserialize)] #[serde(default)]` with an `impl Default` so missing keys get defaults and unknown keys are ignored.
- Write with `toml::to_string_pretty`. Use single-quoted TOML literal strings for Windows paths so backslashes are not escaped — or just let `toml` escape them; both must parse. Test the round-trip.
- Save the config only when `last_save_as_dir` changes or when creating the defaults. Never rewrite it on every capture.

### 4.8 Autostart (`autostart.rs`)

`HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, value name `Skrinka`, data `"<full path to current_exe>"` (quoted). `is_enabled()` = value exists. Toggling from the tray updates the registry and the check state. Errors → log + MessageDialog.

## 5. Threading rules (do not violate)

- Main thread: winit loop, tray, hotkeys, overlay windows, config mutation.
- Worker threads: PNG encoding, clipboard, Save-as dialog, MessageDialogs about output errors, the 300 ms tray delay.
- Workers never touch `App`; they report back through `EventLoopProxy::send_event`.
- The overlay must never be created or dropped from a worker.

## 6. Coordinates, DPI, capture — the part that usually goes wrong

### 6.1 One coordinate space

`winit` 0.30 makes the process **Per-Monitor DPI Aware V2** when the event loop is built (`EventLoopBuilder` default `dpi_aware = true` — leave it). Consequence: every Win32 call we make (`GetSystemMetrics`, `BitBlt`, `GetCursorPos`, `GetMonitorInfo` inside winit) returns **physical pixels**, and `winit` `MonitorHandle::position()/size()`, `PhysicalPosition` in `CursorMoved`, and window positions are physical pixels too. Never multiply or divide by `scale_factor()` anywhere. Never use `LogicalPosition/LogicalSize`.

Monitor origins can be negative (a monitor placed left of/above the primary one). The virtual screen origin `(SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN)` can be negative. Always subtract the capture origin when cropping (§4.5.6).

### 6.2 Capture recipe (`capture.rs`) — GDI, whole virtual screen in one BitBlt

```rust
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN};

pub struct Capture { pub x0: i32, pub y0: i32, pub width: u32, pub height: u32, pub pixels: Vec<u32> } // row-major, 0x00RRGGBB

pub fn capture_virtual_screen() -> anyhow::Result<Capture> {
    unsafe {
        let x0 = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y0 = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w  = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h  = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        anyhow::ensure!(w > 0 && h > 0, "virtual screen size is {w}x{h}");

        let screen_dc = GetDC(None);                       // HDC of the whole virtual screen
        anyhow::ensure!(!screen_dc.is_invalid(), "GetDC failed");
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp.into());
        let blt = BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), x0, y0, SRCCOPY | CAPTUREBLT);

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,            // negative = top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0, // field is u32; BI_RGB is a newtype
            ..Default::default()
        };
        let mut pixels = vec![0u32; (w as usize) * (h as usize)];
        let lines = GetDIBits(mem_dc, bmp, 0, h as u32,
                              Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
                              &mut info, DIB_RGB_COLORS);

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        blt?;                                   // windows_core::Result<()>
        anyhow::ensure!(lines == h, "GetDIBits returned {lines} of {h} lines");
        for p in &mut pixels { *p &= 0x00FF_FFFF; }   // clear the X byte, defensive
        Ok(Capture { x0, y0, width: w as u32, height: h as u32, pixels })
    }
}
```

Signature facts for `windows` 0.62 (verified from the crate source): `GetDC(Option<HWND>) -> HDC`, `CreateCompatibleDC(Option<HDC>) -> HDC`, `BitBlt(..) -> windows_core::Result<()>`, `GetDIBits(hdc, hbm, start, clines, Option<*mut c_void>, *mut BITMAPINFO, DIB_USAGE) -> i32`, `ReleaseDC(Option<HWND>, HDC) -> i32`, `ROP_CODE` implements `BitOr`, `impl From<HBITMAP> for HGDIOBJ` exists (so `.into()` works), `HDC::is_invalid()` exists, `BITMAPINFOHEADER` derives `Default`, `BITMAPINFO` does **not** derive `Default` but has a manual `impl Default`. `GetCursorPos(*mut POINT) -> windows_core::Result<()>`. Single-instance: `CreateMutexW(None, false, windows::core::w!("Local\\Skrinka.SingleInstance")) -> Result<HANDLE>`, then `GetLastError() == ERROR_ALREADY_EXISTS` (both in `Win32::Foundation`).

Pixel format: GDI 32-bit DIB is `B,G,R,X` in memory, which read as a little-endian `u32` is `0x00RRGGBB` — **exactly what `softbuffer` wants**, so `base`/`dimmed` need no conversion. Only when building the `RgbaImage` for saving/clipboard do you unpack to `[r, g, b, 255]`.

`CAPTUREBLT` is required so layered/transparent windows are included. Expect a tiny cursor flicker — normal.

### 6.3 Which monitor is "under the cursor"

`GetCursorPos(&mut POINT)` (physical) → first monitor where `mx <= px < mx+mw && my <= py < my+mh`.

## 7. Pitfalls checklist (each one has bitten real screenshot tools)

1. **Hotkey Released events** — filter `HotKeyState::Pressed` (§3.1).
2. **Overlay in the screenshot** — capture before creating windows; 300 ms delay after a tray-menu click.
3. **Keyboard layout:** the user switches between English and Ukrainian layouts. With the Ukrainian layout active, `logical_key` for the physical `C` key is `с` and `Ctrl+C` would never match. Use `physical_key == PhysicalKey::Code(KeyCode::KeyC)` (§4.5.3). Same for `KeyS`.
4. **Alt is still held** when the overlay opens (the hotkey is Shift+Alt+2). Ignore modifier key events; do not treat an `Alt` release as anything. Windows may interpret a bare Alt release as "enter menu mode" only for windows with a menu bar — ours has none, so nothing to do.
5. **Focus:** the process that received the hotkey is allowed to take the foreground, so `focus_window()` works; if it ever does not, the first click on the overlay focuses it and that is acceptable. Do not add `SetForegroundWindow` hacks.
6. **DPI:** never touch `scale_factor()`; ignore `ScaleFactorChanged` (§6.1). The pre-sized overlay must not be resized by anyone.
7. **Negative coordinates** on multi-monitor setups — all math is `i32`, crop after subtracting the capture origin.
8. **Invalid strftime template panics** in chrono — validate first (§4.6.1).
9. **Blocking dialogs on the event-loop thread** — Save-as and error dialogs run on worker threads (§5).
10. **Modal re-entrancy**: never call `rfd` from inside a winit event callback.
11. **Tray icon lifetime** — keep the `TrayIcon` in `App`, otherwise it disappears immediately.
12. **Hotkey manager lifetime** — same; dropping it unregisters the hotkeys.
13. **Release build has no stdout** — everything goes to the log file; make sure the panic hook is installed before anything else.
14. **Windows "Alt+Shift switches keyboard layout"** is a system shortcut; the user has lived with it under ScreenCloud, so keep the defaults, but make the hotkeys configurable (they are).
15. **A second instance** must not steal the hotkeys — the mutex guard runs before anything else.
16. **PNG encode time** for a 4K-ish region is ~100–300 ms — that is why it is on a worker thread; the clipboard is set first.

## 8. Implementation order — build and run `cargo build --release` after every step; do not move on with warnings

1. Project skeleton, `Cargo.toml` exactly as in §2, `main.rs` with mutex guard + logging + config load + an empty winit loop that exits on a `Quit` tray item. **Checkpoint:** icon in tray, Quit works, log file written.
2. `capture.rs` + a debug-only menu item "Debug: dump capture" that saves the whole virtual screen to `save_dir/debug_capture.png` (remove the item before finishing, or keep it behind `cfg!(debug_assertions)`). **Checkpoint:** PNG shows both monitors, correct size in physical pixels, colours not swapped (red is red).
3. Full-screen hotkey end-to-end (§4.4) + output worker (§4.6) + filename template with unit tests. **Checkpoint:** `Shift+Alt+1` → file appears, clipboard pastes into Paint.
4. Overlay: windows, dimming, drag rectangle, border, `Esc`/right-click cancel, `Enter` finish. **Checkpoint:** drag on each monitor works, saved image matches what was selected pixel-for-pixel (check edges).
5. Labels/help text via fontdue. `Ctrl+C`, `Ctrl+S` Save-as flow, `last_save_as_dir` persistence.
6. Remaining tray items (open folder, open config, reload, autostart).
7. README.md (how to build, config keys, hotkeys, known limitations: no cross-monitor selection, no editor yet). Final `cargo clippy --release -- -D warnings`, `cargo test`.

Commit after each step with a short message.

## 9. Acceptance tests

### 9.1 Manual (the user runs these; list them in README as a checklist)

1. Start → tray icon appears; second start does nothing.
2. `Shift+Alt+1` with the cursor on monitor 1 → PNG of monitor 1 only; on monitor 2 → monitor 2 only. Paste in Paint works.
3. `Shift+Alt+2` → both monitors dim; help text visible; crosshair cursor.
4. Drag on monitor 2 → region is bright, red border, size label updates while dragging.
5. `Enter` → overlay closes, file in `save_dir`, clipboard has the image; the file content equals the selected area (compare a known UI element at the edges — off-by-one or DPI scaling bugs show up here).
6. `Esc` and right-click cancel with no file written.
7. `Ctrl+S` → Save dialog opens in `save_dir` the first time with the generated name; choose another folder → next `Ctrl+S` opens there.
8. `Ctrl+C` → clipboard only, nothing written.
9. With Ukrainian keyboard layout active: `Ctrl+C`, `Ctrl+S`, `Enter`, `Esc` still work in the overlay.
10. Edit `filename_format` to something invalid (`%Q%`) → Reload → capture still works (default name), warning in log.
11. "Run on startup" toggles the registry value (check `regedit` or Task Manager → Startup).
12. Memory of the idle process stays small (expect < 20 MB); exe size a few MB.

### 9.2 Automated (`cargo test`)

- `format_filename`: default template produces `Screenshot YYYY-MM-DD HH-MM-SS.png`; invalid template falls back; sanitization removes `:` etc.; `.png` appended once; `unique_path` yields `(2)`, `(3)`.
- `Rect::normalize` from any two corners; inclusive w/h; clamping.
- Crop math: given capture origin `(-1920, 0)`, monitor origin `(-1920, 0)`, rect `(10,20,30,40)` → capture crop `(10,20,30,40)`; monitor origin `(0,0)` → `(1930,20,30,40)`.
- Config round-trip with a Windows path containing backslashes.
- 0x00RRGGBB → RGBA conversion on a known pixel.

## 10. Later phases (for orientation only — do not implement now)

- v2: move/resize the selection with handles; "capture active window" hotkey (`Shift+Alt+3`); Windows toast/balloon after save; `{rnd}` token in the template.
- v3: simple editor (arrow, rectangle, text, pixelate) opened with `Ctrl+E` from the overlay.
- Maybe: second instance forwards a "capture" command to the running one (ScreenCloud 1.5.2 had this).
