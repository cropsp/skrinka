# TASK v2: Skrinka 0.2 — selection handles, window capture, notifications

> Read `CLAUDE.md`, `TASK.md` (the v1 spec) and `README.md` first. v1 is finished and tagged; this
> task adds three features on top of the existing code **without rewriting it**. Everything in
> `TASK.md` §6 (coordinates/DPI), §5 (threading) and §7 (pitfalls) still applies. Every API name
> and signature below was checked against the exact crate versions in `Cargo.lock`.

## 0. Scope

### In scope (v2)

1. **Selection handles** in the region overlay: move the selection by dragging inside it, resize it
   by dragging its edges and corners, nudge it with the arrow keys. (ScreenCloud parity.)
2. **Capture active window** on `Shift+Alt+3`, also from the tray menu.
3. **Notification after a save**: a native tray balloon ("toast") saying which file was written.
4. Housekeeping: git tags, version bump, README and `CLAUDE.md` updates.

### Still out of scope (do not implement)

Editor (arrows/text/blur — that is v3, a separate task), uploads, JPG, settings GUI, CLI arguments,
a selection spanning two monitors, click-to-open on the notification. **Never auto-update or
download anything.**

## 1. Before touching code

```
git status                       # must be clean
git tag -a v0.1.0 -m "Skrinka 0.1.0: region/full-screen capture, tray, hotkeys"
```

Then work on `main` in the same step-by-step, commit-per-step style as v1. Build with
`cargo build --release`, keep `cargo clippy --release --all-targets -- -D warnings` clean and
`cargo test` green after every step. Stop the running `skrinka.exe` before rebuilding (a copy
running from the folder you installed it in does not lock `target\release\skrinka.exe`, but a copy
started from `target\release` would).

## 2. Feature A — selection handles (`src/overlay.rs`, `src/util.rs`)

### 2.1 Behaviour

The selection is an inclusive pixel rect `r` on one window (`Selection { win, rect }`, unchanged).
Let `left = r.x`, `right = r.x + r.w - 1`, `top = r.y`, `bottom = r.y + r.h - 1`.

**Hit test** of a window-local point `p` against the selection *of that same window* (a selection
on another window counts as `Outside`), with `HANDLE_HIT: i32 = 8`:

```
near_left   = (p.x - left).abs()   <= HANDLE_HIT
near_right  = (p.x - right).abs()  <= HANDLE_HIT
near_top    = (p.y - top).abs()    <= HANDLE_HIT
near_bottom = (p.y - bottom).abs() <= HANDLE_HIT
within_x    = left - HANDLE_HIT <= p.x && p.x <= right + HANDLE_HIT
within_y    = top  - HANDLE_HIT <= p.y && p.y <= bottom + HANDLE_HIT

if (near_left || near_right) && (near_top || near_bottom) → Corner (TopLeft/TopRight/BottomLeft/BottomRight,
                                                            left wins over right, top over bottom when both are near)
else if (near_left || near_right) && within_y             → Edge(Left | Right)
else if (near_top || near_bottom) && within_x             → Edge(Top | Bottom)
else if r.contains(p)                                      → Inside
else                                                       → Outside
```

```rust
enum Hit { Outside, Inside, Edge(Side), Corner(Corner) }
enum Side { Left, Right, Top, Bottom }
enum Corner { TopLeft, TopRight, BottomLeft, BottomRight }
```

**Drag kinds** — replace the current `Drag { win, start, cur }` with:

```rust
enum DragKind {
    Draw,                                   // today's behaviour: rect = from_corners(start, cur)
    Move { grab: (i32, i32) },              // cursor offset from the selection's top-left at press time
    Resize { fixed: (i32, i32), moving: (i32, i32), move_x: bool, move_y: bool },
}
struct Drag { win: usize, kind: DragKind, start: (i32, i32), cur: (i32, i32) }
```

**Left press** at `p` on window `idx` (replaces the current unconditional "start drawing"):

| Hit | Drag started | Selection |
|---|---|---|
| `Outside` | `Draw`, `start = cur = p`, `started = true` | cleared (`None`) |
| `Inside` | `Move { grab: (p.x - left, p.y - top) }` | kept |
| `Corner`/`Edge` | `Resize { .. }` per the table below | kept |

Resize parameters (`fixed` is the corner that stays, `moving` the one that follows the cursor;
an edge freezes one axis):

| Grabbed | `fixed` | `moving` | `move_x` | `move_y` |
|---|---|---|---|---|
| Corner TopLeft | (right, bottom) | (left, top) | true | true |
| Corner TopRight | (left, bottom) | (right, top) | true | true |
| Corner BottomLeft | (right, top) | (left, bottom) | true | true |
| Corner BottomRight | (left, top) | (right, bottom) | true | true |
| Edge Left | (right, bottom) | (left, top) | true | false |
| Edge Right | (left, top) | (right, bottom) | true | false |
| Edge Top | (right, bottom) | (left, top) | false | true |
| Edge Bottom | (left, top) | (right, bottom) | false | true |

**Live rect** while dragging (`live_rect(idx)`), with `cur` already clamped to the window:

- `Draw` → `Rect::from_corners(start, cur)` (unchanged).
- `Move { grab }` → `x = (cur.x - grab.x).clamp(0, W - r.w)`, `y = (cur.y - grab.y).clamp(0, H - r.h)`,
  size unchanged (`W`,`H` = window size).
- `Resize { fixed, moving, move_x, move_y }` → `nx = if move_x { cur.x } else { moving.x }`,
  `ny = if move_y { cur.y } else { moving.y }`, `Rect::from_corners(fixed, (nx, ny))`.
  Crossing over the fixed corner flips the rectangle naturally; `fixed` never changes during a drag.

**Left release** on the dragging window: `selection = Some(live)`, `drag = None`, except a `Draw`
whose `start == cur` (a plain click outside) → `selection = None`. Then re-run the cursor update
below and redraw that window.

**Cursor** (`window.set_cursor`, `winit::window::CursorIcon`; names verified in `cursor-icon` 1.x):
on every `CursorMoved` while `drag.is_none()`, map the hit → `Outside: Crosshair`, `Inside: Move`,
`Edge(Left|Right): EwResize`, `Edge(Top|Bottom): NsResize`, `Corner(TopLeft|BottomRight): NwseResize`,
`Corner(TopRight|BottomLeft): NeswResize`. Keep `current_cursor: CursorIcon` in `OverlayWindow` and
call `set_cursor` only when it changes. During a drag the cursor stays as it was at press time.

**Arrow keys** (physical `KeyCode::ArrowLeft/Right/Up/Down`), only when `selection.is_some()` and
`drag.is_none()`: move the selection by 1 px, or 10 px with Shift held (track `shift` from
`ModifiersChanged` like `ctrl`), clamped inside its window; redraw that window. Arrow keys may
repeat (`event.repeat` allowed); Enter/C/S/Esc keep the `!event.repeat` filter. The nudge acts on
the selection's window even if another overlay window has the keyboard focus.

Esc, Enter, Ctrl+C, Ctrl+S and right-click keep their v1 meaning. `finish()` still ignores a
request while any drag is in progress and still requires `w >= 2 && h >= 2`.

### 2.2 Rendering

After the border, draw **8 handles**: filled squares of `HANDLE_SIZE: i32 = 8` in `BORDER_COLOR`,
centred on the four corners and the four edge midpoints of the live rect, clipped to the window
(reuse `fill_rect`). Show them whenever a live rect exists and the current drag is not `Draw`
(i.e. for a finished selection and during Move/Resize; not while drawing a new one).

Update `HELP` to:
`Drag to select  ·  Drag inside to move, edges to resize  ·  Arrows: nudge  ·  Enter: copy + save  ·  Ctrl+S: save as  ·  Ctrl+C: copy  ·  Esc: cancel`

### 2.3 Unit tests (pure functions, no windows)

Put `hit_test`, the resize-parameter table and the live-rect math in free functions so they are
testable:
- hit test: inside, each edge, each corner, 8 px outside an edge → `Edge`, 9 px outside → `Outside`,
  a 3×3 selection where everything is a corner;
- resize from BottomRight dragged past the TopLeft flips (rect has the fixed corner as bottom-right);
- Edge Right drag never changes `y`/`h`;
- move clamps to the window on all four sides;
- nudge clamps.

## 3. Feature B — capture active window (`src/window.rs` new, `app.rs`, `hotkeys.rs`, `tray.rs`, `config.rs`)

### 3.1 Config

```toml
hotkey_window = 'Shift+Alt+3'
window_capture = 'visible'      # 'visible' (what you see: frame + title bar) | 'client' (contents only, no title bar)
```
`DEFAULT_HOTKEY_WINDOW = "Shift+Alt+3"`, `enum WindowCapture { Visible, Client }` with
`#[serde(rename_all = "snake_case")]`, default `Visible`; `#[serde(default)]` keeps old config files valid.

### 3.2 Finding the window — `window::foreground_window(mode) -> Option<ForegroundWindow>`

```rust
pub struct ForegroundWindow { pub rect: Rect, pub title: String, pub maximized: bool }  // rect: virtual-screen physical px
```

All in `windows::Win32::UI::WindowsAndMessaging` unless noted; signatures verified in `windows` 0.62.2:

1. `GetForegroundWindow() -> HWND`; null (`is_invalid()`) → `None`.
2. `IsIconic(hwnd) -> BOOL` true (minimized) → `None`.
3. `GetWindowThreadProcessId(hwnd, Some(&mut pid)) -> u32`; if `pid == GetCurrentProcessId()`
   (`Win32::System::Threading`) → `None` (never capture Skrinka's own dialogs).
4. `Visible` mode: `DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut rect as *mut RECT as *mut c_void, size_of::<RECT>() as u32) -> Result<()>`
   (`Win32::Graphics::Dwm`, cargo feature `Win32_Graphics_Dwm`). This is the window as drawn,
   without Windows 10/11's invisible resize borders. On `Err` fall back to `GetWindowRect(hwnd, &mut rect) -> Result<()>`.
   `Client` mode: `GetClientRect(hwnd, &mut rect) -> Result<()>` gives (0,0,w,h); convert the origin
   with `ClientToScreen(hwnd, &mut POINT{0,0}) -> BOOL` (`Win32::Graphics::Gdi`).
5. `RECT {left, top, right, bottom}` is half-open → `Rect::new(left, top, right - left, bottom - top)`;
   `w <= 0 || h <= 0` → `None`.
6. `maximized = IsZoomed(hwnd).as_bool()`.
7. Title for the log only: `GetWindowTextW(hwnd, &mut [0u16; 256]) -> i32` (length), lossy UTF-16 decode.

Because the process is per-monitor DPI aware, all of these are physical pixels — same space as
the capture and winit's monitors. Do not scale anything.

### 3.3 Capturing — `App::capture_window(event_loop)`

1. Ignore if an overlay is open (like the other captures).
2. `capture_virtual_screen()` first, then `foreground_window(self.config.window_capture)`.
3. Target rect = window rect → if `maximized`, intersect with the monitor containing the rect's
   centre (`event_loop.available_monitors()` → `Rect`s; a maximized window's frame bounds can stick
   out of the monitor by the border width) → `capture.screen_to_capture(..)` → intersect with
   `capture.rect()`.
4. If any step yields `None` → log a warning and fall back to the cursor-monitor full-screen
   capture (`cursor_monitor(event_loop)`), so the hotkey always produces something.
5. `util::crop_rgba` → `output::spawn(image, Action::ClipboardAndAutoSave, ..)`. Log the title and rect.

The capture is screen-based: another window overlapping the active one is in the picture, exactly
like ScreenCloud. Say so in README. `PrintWindow` is **not** used (unreliable for GPU-rendered apps).

### 3.4 Plumbing

- `hotkeys.rs`: `Ids { region, fullscreen, window }`, `UserEvent::HotkeyWindow`, third
  `register_one("hotkey_window", ..)`, `window_label`. Same shared-ids mechanism; the handler is
  still installed once.
- `tray.rs`: new `MenuItem` "Capture window" right after "Capture full screen"; `set_hotkey_labels`
  gets a third parameter. `CaptureKind::Window`, `TrayAction::Capture(CaptureKind::Window)`,
  `DelayedCapture(CaptureKind::Window)` with the existing 300 ms delay. From the menu, the foreground
  window is whatever Windows reactivates after the menu closes (normally the previously active
  window); note this in README.
- `Cargo.toml` `windows` features: add `"Win32_Graphics_Dwm"`.

### 3.5 Tests

- Unit: RECT → Rect conversion, maximized-window clamp to its monitor, fallback when the rect is
  outside the capture.
- End to end (the approach in `CLAUDE.md` "Verifying behaviour"): start `notepad.exe`, place it with
  `SetWindowPos` at a known rect on monitor 2, give it focus, inject `Shift+Alt+3`, then compare the
  new PNG's size with the DWM frame bounds read for that window, and compare the pixels against a
  full-screen PNG at the logged rect (0 differing pixels expected, as in the v1 region test).

## 4. Feature C — notification after a save (`src/tray.rs`, `app.rs`, `config.rs`, `output.rs`)

### 4.1 Mechanism: a tray balloon via `Shell_NotifyIconW(NIM_MODIFY)` with `NIF_INFO`

On Windows 10/11 a tray balloon is shown as a normal toast notification and needs no app
registration, no AppUserModelID and no extra crate. `tray-icon` does not expose balloons, but the
icon it created can be addressed directly:

- `hWnd` = `HWND(self._icon.window_handle())` — `TrayIcon::window_handle()` exists on Windows
  (`tray-icon` 0.26.1, `src/lib.rs`), returns `windows_sys` `HWND` (a `*mut c_void`), so wrap it in
  `windows::Win32::Foundation::HWND(..)`.
- `uID` = **1**. `tray-icon` numbers icons from a process-wide counter that starts at 1
  (`tray-icon-0.26.1/src/counter.rs`: `AtomicU32::new(1)`, `fetch_add`), and Skrinka creates exactly
  one tray icon. This is an internal detail of a pinned version, so protect it:
  - put it in one place: `const TRAY_ICON_UID: u32 = 1;` with this explanation in a comment;
  - **verify at startup**: `Shell_NotifyIconGetRect(&NOTIFYICONIDENTIFIER { cbSize, hWnd, uID: TRAY_ICON_UID, guidItem: GUID::zeroed() }) -> Result<RECT>`
    must succeed and equal the rect from `self.icon.rect()` (`tray_icon::Rect { position: PhysicalPosition<f64>, size: PhysicalSize<u32> }`,
    already physical px; compare `position` rounded and `size`). Log `info!("tray icon id verified")`
    or `warn!(..)` and, on mismatch, disable notifications for the session (`notifications_ok: Cell<bool>`).
  - The field is currently named `_icon` because nothing used it; rename it to `icon`.

`Tray::notify(&self, title: &str, text: &str)` (call only from the main thread, where the tray lives):

```rust
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_INFO, NIIF_NONE, NIIF_NOSOUND, NIM_MODIFY, NOTIFYICONDATAW};
let mut nid = NOTIFYICONDATAW {
    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
    hWnd: HWND(self._icon.window_handle()),
    uID: TRAY_ICON_UID,
    uFlags: NIF_INFO,
    dwInfoFlags: NIIF_NONE | NIIF_NOSOUND,     // Windows shows the app's own tray icon; no sound (ScreenCloud default)
    ..Default::default()                        // Default = zeroed, exists for x86_64
};
copy_utf16(&mut nid.szInfoTitle, title);        // [u16; 64]: at most 63 units + NUL
copy_utf16(&mut nid.szInfo, text);              // [u16; 256]: at most 255 units + NUL
let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool();
if !ok { warn once; disable for the session }
```

Cargo features: add `"Win32_UI_Shell"` (`NOTIFYICONDATAW` also needs `Win32_UI_WindowsAndMessaging`,
already enabled). `NIF_INFO: NOTIFY_ICON_DATA_FLAGS`, `NIIF_*: NOTIFY_ICON_INFOTIP_FLAGS` — both
implement `BitOr`. Truncate titles/texts on UTF-16 unit boundaries, never on bytes (file names can
be Cyrillic).

### 4.2 When and what

```toml
notifications = 'saves'   # 'saves' (default) | 'all' | 'off'
```

In `App::on_worker_done`, only when `result.error.is_none()`:

| Action | `saves` | `all` |
|---|---|---|
| `ClipboardAndAutoSave`, `SaveAs` with `saved_path` | title `Screenshot saved`, text `<file name>\n<folder>` | same |
| `SaveAs` cancelled (`saved_path == None`) | nothing | nothing |
| `ClipboardOnly` | nothing | title `Copied to clipboard`, text `<W> × <H>` |

Add `pub size: (u32, u32)` to `WorkerResult` (set in `output::spawn` before the image is dropped).
Errors already show a dialog; no balloon for them.

README caveats to mention: the toast appears only if notifications for the app are allowed in
Windows Settings and Focus Assist / Do Not Disturb is off; clicking it does nothing; it also lands
in the notification centre.

## 5. Housekeeping (last step)

- `Cargo.toml` `version = "0.2.0"`; the log line at startup already prints the version.
- README: hotkey table (+`Shift+Alt+3`), overlay keys (+ move/resize/arrows), config keys
  (+`hotkey_window`, `window_capture`, `notifications`), "Known limitations" (remove "cannot be
  moved or resized"; add overlap note for window capture; add notification caveats), the manual
  checklist (+ items from §6.1), "How it works" (+ DWM frame bounds, the tray uID assumption and
  its startup check).
- `CLAUDE.md`: the first paragraph now says `TASK.md` is the v1 spec and `TASK-v2.md` the v2 spec;
  selection handles, window capture and notifications are in scope; the rest of the out-of-scope
  list stays; "never auto-update" stays. Add to Architecture: the hit-test/drag model, the DWM rect
  rule for maximized windows, the tray uID = 1 assumption and where it is verified.
- Final: `cargo clippy --release --all-targets -- -D warnings`, `cargo test`, `cargo build --release`,
  commit, `git tag -a v0.2.0 -m "Skrinka 0.2.0: selection handles, window capture, notifications"`.
- Tell the user to copy `target\release\skrinka.exe` over the copy in the folder they installed it
  in (quit the running copy first).

## 6. Acceptance

### 6.1 Manual (append to the README checklist)

1. Draw a selection, hover inside → move cursor; drag → it follows, cannot leave the monitor.
2. Hover an edge → resize cursor; drag the right edge left past the left edge → the rect flips, never collapses to nothing weird.
3. Drag a corner; the size label updates; handles stay centred on corners/edge midpoints.
4. Arrow keys move the selection by 1 px, Shift+arrows by 10 px; it stops at the monitor edge.
5. Click outside the selection without moving → selection disappears, help text does not come back (`started` stays true).
6. Enter after moving/resizing saves exactly the shown rectangle (edge check as in v1).
7. `Shift+Alt+3` on a normal window → PNG of that window including the title bar, no desktop around it; on a maximized window → the whole monitor minus nothing extra (no black/other-monitor strip at the edges).
8. `window_capture = 'client'` + Reload → the PNG has no title bar.
9. `Shift+Alt+3` with the desktop focused or with nothing sensible in front → still produces a capture (fallback) and the log says why.
10. After Enter / Shift+Alt+1 / Shift+Alt+3 a toast "Screenshot saved — <name>" appears; `notifications = 'off'` + Reload → none; `'all'` → also after Ctrl+C.
11. Idle memory after the above stays at the v1 level (about 12 MB / 3 MB private after region captures).
12. Ukrainian layout: arrows, Ctrl+C/S, Enter, Esc still work.

### 6.2 Implementation order

1. Tag `v0.1.0`. Hit test + drag kinds + live rect + unit tests (no rendering yet) → build.
2. Cursors, handles, arrows, HELP text → manual check by the user.
3. `window.rs` + config + hotkey + tray item → end-to-end test with Notepad.
4. Notifications + startup uID check → check the log line and a visible toast.
5. README, `CLAUDE.md`, version, tag `v0.2.0`.

Commit after each step.
