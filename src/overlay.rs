//! Region selection: a frozen, dimmed copy of every monitor in one borderless window per
//! monitor. A selection lives on one monitor; it cannot span two.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::thread::JoinHandle;

use anyhow::Context as _;
use image::RgbaImage;
use log::{error, info, warn};
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
use winit::window::{CursorIcon, Window, WindowId, WindowLevel};

use crate::capture::{self, Capture};
use crate::output::Action;
use crate::strings;
use crate::text::Text;
use crate::util::{self, Rect};

const BORDER_COLOR: u32 = 0x00E3_4133; // ScreenCloud red (227, 65, 51)
const BORDER_WIDTH: i32 = 2;
const HELP: &str = strings::OVERLAY_HELP;
const HELP_TOP: i32 = 40;
/// Gap between the size label and the selection below it.
const LABEL_GAP: i32 = 4;
/// How far from an edge of the selection a press still grabs that edge.
const HANDLE_HIT: i32 = 8;
/// Side of the squares drawn on the corners and edge midpoints of the selection.
const HANDLE_SIZE: i32 = 8;

pub enum Outcome {
    Continue,
    Cancel,
    Finish(RgbaImage, Action),
}

struct OverlayWindow {
    window: Rc<Window>,
    _context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    /// The monitor in virtual-screen pixels; the window covers exactly this.
    monitor: Rect,
    /// The monitor's part of the capture, `0x00RRGGBB`.
    base: Vec<u32>,
    /// `base` at 45 % brightness.
    dimmed: Vec<u32>,
    /// Window-local, clamped; `MouseInput` carries no position.
    last_cursor: (i32, i32),
    /// What `set_cursor` was last called with.
    current_cursor: CursorIcon,
}

/// Where a window-local point lies relative to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Outside,
    Inside,
    Edge(Side),
    Corner(Corner),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragKind {
    /// A new selection spanned by the press point and the cursor.
    Draw,
    /// The selection follows the cursor; `grab` is the cursor's offset from its top-left corner
    /// at press time.
    Move { grab: (i32, i32) },
    /// `fixed` is the corner that stays, `moving` the one that follows the cursor on the axes
    /// that move; an edge freezes one axis.
    Resize { fixed: (i32, i32), moving: (i32, i32), move_x: bool, move_y: bool },
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    win: usize,
    kind: DragKind,
    start: (i32, i32),
    cur: (i32, i32),
}

#[derive(Debug, Clone, Copy)]
struct Selection {
    win: usize,
    rect: Rect,
}

pub struct OverlaySession {
    capture: Capture,
    text: Text,
    windows: Vec<OverlayWindow>,
    drag: Option<Drag>,
    selection: Option<Selection>,
    ctrl: bool,
    shift: bool,
    /// Set by the first mouse press; hides the help line.
    started: bool,
}

impl OverlaySession {
    /// `capture` must be taken before this is called, so the overlay is not in it. `text` is
    /// the font being loaded meanwhile on another thread.
    pub fn open(
        event_loop: &ActiveEventLoop,
        capture: Capture,
        text: JoinHandle<Text>,
    ) -> anyhow::Result<OverlaySession> {
        let monitors: Vec<Rect> = event_loop
            .available_monitors()
            .map(|m| {
                let (pos, size) = (m.position(), m.size());
                Rect::new(pos.x, pos.y, size.width as i32, size.height as i32)
            })
            .filter(|r| r.w > 0 && r.h > 0)
            .collect();
        anyhow::ensure!(!monitors.is_empty(), "no monitors found");
        let cursor = capture::cursor_position();
        let focused = cursor.and_then(|p| util::find_containing(&monitors, p)).unwrap_or(0);

        let mut windows = Vec::with_capacity(monitors.len());
        for (i, &monitor) in monitors.iter().enumerate() {
            let win = OverlayWindow::new(event_loop, &capture, monitor, i == focused, cursor)
                .with_context(|| format!("cannot create the overlay for monitor {monitor:?}"))?;
            windows.push(win);
        }
        let text = text.join().unwrap_or_else(|_| {
            error!("loading the overlay font panicked");
            Text::none()
        });

        // Shown only now, with the monitor under the cursor last: it is the one created
        // "active", so it gets the keyboard focus. winit's `focus_window()` is not used because
        // it injects an Alt press, which together with the still-held Shift of the hotkey can
        // switch the keyboard layout.
        for (i, win) in windows.iter().enumerate() {
            if i != focused {
                win.window.set_visible(true);
            }
        }
        windows[focused].window.set_visible(true);
        for win in &windows {
            win.window.request_redraw();
        }
        if !windows[focused].window.has_focus() {
            info!("winit reports no keyboard focus on the overlay yet; a click will focus it");
        }
        info!("region overlay open on {} monitor(s)", windows.len());

        Ok(OverlaySession {
            capture,
            text,
            windows,
            drag: None,
            selection: None,
            ctrl: false,
            shift: false,
            started: false,
        })
    }

    pub fn window_event(&mut self, id: WindowId, event: WindowEvent) -> Outcome {
        let Some(idx) = self.windows.iter().position(|w| w.window.id() == id) else {
            return Outcome::Continue;
        };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let win = &mut self.windows[idx];
                win.last_cursor = util::clamp_to_window(position.x, position.y, win.monitor.w, win.monitor.h);
                if let Some(drag) = self.drag.as_mut().filter(|d| d.win == idx) {
                    drag.cur = win.last_cursor;
                    win.window.request_redraw();
                } else {
                    self.update_cursor(idx);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let at = self.windows[idx].last_cursor;
                let kind = press_kind(self.selection_on(idx), at);
                self.drag = Some(Drag { win: idx, kind, start: at, cur: at });
                self.started = true;
                if kind == DragKind::Draw {
                    self.selection = None;
                    // Also clears a selection on another monitor and the help line everywhere.
                    self.redraw_all();
                } else {
                    self.windows[idx].window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                if let Some(drag) = self.drag.filter(|d| d.win == idx) {
                    let rect = drag.rect(self.selection_on(idx), self.windows[idx].size());
                    self.drag = None;
                    // A plain click outside the selection leaves nothing selected.
                    let click = drag.kind == DragKind::Draw && drag.start == drag.cur;
                    self.selection = (!click).then_some(Selection { win: idx, rect });
                    self.update_cursor(idx);
                    self.windows[idx].window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                return Outcome::Cancel;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.ctrl = modifiers.state().control_key();
                self.shift = modifiers.state().shift_key();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let step = if self.shift { 10 } else { 1 };
                // Physical keys: with the Ukrainian layout the logical key for C is "с".
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowLeft) => self.nudge_selection(-step, 0),
                    PhysicalKey::Code(KeyCode::ArrowRight) => self.nudge_selection(step, 0),
                    PhysicalKey::Code(KeyCode::ArrowUp) => self.nudge_selection(0, -step),
                    PhysicalKey::Code(KeyCode::ArrowDown) => self.nudge_selection(0, step),
                    // Only the arrows act on key repeats.
                    _ if event.repeat => {}
                    PhysicalKey::Code(KeyCode::Escape) => return Outcome::Cancel,
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                        return self.finish(Action::ClipboardAndAutoSave);
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if self.ctrl => return self.finish(Action::ClipboardOnly),
                    PhysicalKey::Code(KeyCode::KeyS) if self.ctrl => return self.finish(Action::SaveAs),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.render(idx),
            WindowEvent::CloseRequested => return Outcome::Cancel,
            WindowEvent::ScaleFactorChanged { mut inner_size_writer, .. } => {
                // winit would resize the window to keep its logical size; it must stay exactly
                // monitor-sized in physical pixels.
                let m = self.windows[idx].monitor;
                let _ = inner_size_writer.request_inner_size(PhysicalSize::new(m.w as u32, m.h as u32));
            }
            WindowEvent::Resized(_) => {
                // Resize events from window creation (Windows' default size) arrive late, after
                // the window already has its final size, so the current size is what counts.
                let win = &self.windows[idx];
                let size = win.window.inner_size();
                if (size.width as i32, size.height as i32) != (win.monitor.w, win.monitor.h) {
                    warn!("overlay for {:?} is now {}x{}", win.monitor, size.width, size.height);
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn finish(&mut self, action: Action) -> Outcome {
        // Ignored while the mouse button is still held.
        if self.drag.is_some() {
            return Outcome::Continue;
        }
        let Some(sel) = self.selection.filter(|s| s.rect.w >= 2 && s.rect.h >= 2) else {
            return Outcome::Continue;
        };
        let monitor = self.windows[sel.win].monitor;
        let rect = self.capture.screen_to_capture(sel.rect.offset(monitor.x, monitor.y));
        match util::crop_rgba(&self.capture, rect) {
            Some(image) => {
                info!("region {:?} on monitor {monitor:?} → capture {rect:?}", sel.rect);
                Outcome::Finish(image, action)
            }
            None => {
                error!("selection {rect:?} is outside the capture");
                Outcome::Cancel
            }
        }
    }

    fn redraw_all(&self) {
        for win in &self.windows {
            win.window.request_redraw();
        }
    }

    /// Moves the selection by `(dx, dy)` inside its window, also when another window has the
    /// keyboard focus. Ignored while the mouse button is held.
    fn nudge_selection(&mut self, dx: i32, dy: i32) {
        if self.drag.is_some() {
            return;
        }
        let Some(sel) = self.selection.as_mut() else { return };
        sel.rect = nudge(sel.rect, dx, dy, self.windows[sel.win].size());
        let idx = sel.win;
        self.update_cursor(idx);
        self.windows[idx].window.request_redraw();
    }

    /// Shows on window `idx` what a press at the cursor would do. Left alone during a drag: the
    /// cursor stays as it was at press time.
    fn update_cursor(&mut self, idx: usize) {
        if self.drag.is_some() {
            return;
        }
        let selection = self.selection_on(idx);
        let win = &mut self.windows[idx];
        let icon = cursor_for(selection.map_or(Hit::Outside, |r| hit_test(r, win.last_cursor)));
        if icon != win.current_cursor {
            win.window.set_cursor(icon);
            win.current_cursor = icon;
        }
    }

    /// The selection, if it is on window `idx`.
    fn selection_on(&self, idx: usize) -> Option<Rect> {
        self.selection.filter(|s| s.win == idx).map(|s| s.rect)
    }

    /// The rectangle to show on window `idx`: the one being dragged, else the selection.
    fn live_rect(&self, idx: usize) -> Option<Rect> {
        match self.drag {
            Some(d) if d.win == idx => Some(d.rect(self.selection_on(idx), self.windows[idx].size())),
            Some(_) => None,
            None => self.selection_on(idx),
        }
    }

    fn render(&mut self, idx: usize) {
        let live = self.live_rect(idx);
        // Not while a new selection is being drawn.
        let handles = !matches!(self.drag, Some(Drag { kind: DragKind::Draw, .. }));
        let text = &self.text;
        let win = &mut self.windows[idx];
        let (w, h) = (win.monitor.w, win.monitor.h);
        let mut buffer = match win.surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(e) => {
                error!("overlay buffer: {e}");
                return;
            }
        };
        buffer.copy_from_slice(&win.dimmed);
        if let Some(rect) = live.and_then(|r| r.intersect(&Rect::new(0, 0, w, h))) {
            for row in rect.y..rect.y + rect.h {
                let start = (row * w + rect.x) as usize;
                let end = start + rect.w as usize;
                buffer[start..end].copy_from_slice(&win.base[start..end]);
            }
            draw_border(&mut buffer, w, h, rect);
            if handles {
                draw_handles(&mut buffer, w, h, rect);
            }
            let label = format!("{} × {}", rect.w, rect.h);
            if let Some(size) = text.label_size(&label) {
                text.draw_label(&mut buffer, w, h, label_position(rect, size, w), &label);
            }
        }
        if !self.started {
            if let Some((bw, _)) = text.label_size(HELP) {
                text.draw_label(&mut buffer, w, h, ((w - bw) / 2, HELP_TOP), HELP);
            }
        }
        if let Err(e) = buffer.present() {
            error!("overlay present: {e}");
        }
    }
}

impl OverlayWindow {
    fn new(
        event_loop: &ActiveEventLoop,
        capture: &Capture,
        monitor: Rect,
        active: bool,
        cursor: Option<(i32, i32)>,
    ) -> anyhow::Result<OverlayWindow> {
        let (pos, size) = (
            PhysicalPosition::new(monitor.x, monitor.y),
            PhysicalSize::new(monitor.w as u32, monitor.h as u32),
        );
        let attributes = Window::default_attributes()
            .with_title("Skrinka overlay")
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(false)
            // Hidden until it has its final position and size.
            .with_visible(false)
            .with_position(pos)
            .with_inner_size(size)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_active(active)
            .with_cursor(CursorIcon::Crosshair)
            .with_skip_taskbar(true)
            .with_undecorated_shadow(false)
            .with_corner_preference(CornerPreference::DoNotRound)
            // No file drops. With drag and drop on, winit calls OleInitialize for every window
            // and never undoes it, which keeps OLE's DLLs and threads loaded for the whole run.
            .with_drag_and_drop(false);
        let window = Rc::new(event_loop.create_window(attributes)?);

        // winit creates the window on the primary monitor and then moves it; arriving on a
        // monitor with another scale factor it rescales the window to keep its logical size.
        // Now that it is on the right monitor, set the physical geometry once more.
        window.set_outer_position(pos);
        let _ = window.request_inner_size(size);
        let (actual_pos, actual_size) = (window.outer_position().ok(), window.inner_size());
        if actual_pos != Some(pos) || actual_size != size {
            warn!("overlay for {monitor:?} is at {actual_pos:?} with size {actual_size:?}");
        }

        let context = Context::new(window.clone()).map_err(|e| anyhow::anyhow!("softbuffer: {e}"))?;
        let mut surface =
            Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("softbuffer: {e}"))?;
        let (nw, nh) = (
            NonZeroU32::new(size.width).context("zero-width monitor")?,
            NonZeroU32::new(size.height).context("zero-height monitor")?,
        );
        surface.resize(nw, nh).map_err(|e| anyhow::anyhow!("softbuffer: {e}"))?;

        let base = monitor_pixels(capture, monitor);
        let dimmed = base.iter().map(|&p| dim(p)).collect();
        let last_cursor = cursor.map_or((0, 0), |(x, y)| {
            util::clamp_to_window((x - monitor.x) as f64, (y - monitor.y) as f64, monitor.w, monitor.h)
        });

        Ok(OverlayWindow {
            window,
            _context: context,
            surface,
            monitor,
            base,
            dimmed,
            last_cursor,
            current_cursor: CursorIcon::Crosshair,
        })
    }

    /// Width and height in pixels; the window covers its monitor exactly.
    fn size(&self) -> (i32, i32) {
        (self.monitor.w, self.monitor.h)
    }
}

impl Drag {
    /// The rectangle this drag shows with the cursor at `cur`, inside a window of `(w, h)`.
    /// `selection` is the selection on the drag's window; only `Move` needs it.
    fn rect(&self, selection: Option<Rect>, (w, h): (i32, i32)) -> Rect {
        let (cx, cy) = self.cur;
        match (self.kind, selection) {
            (DragKind::Move { grab: (gx, gy) }, Some(r)) => r.placed_within(cx - gx, cy - gy, w, h),
            (DragKind::Resize { fixed, moving, move_x, move_y }, _) => {
                let nx = if move_x { cx } else { moving.0 };
                let ny = if move_y { cy } else { moving.1 };
                // Crossing the fixed corner flips the rectangle; `fixed` itself never changes.
                Rect::from_corners(fixed, (nx, ny))
            }
            _ => Rect::from_corners(self.start, self.cur),
        }
    }
}

/// Where `p` lies relative to the selection `r`, both in the same window's pixels. Corners win
/// over edges, left over right and top over bottom, so in a tiny selection everything is the
/// top-left corner.
fn hit_test(r: Rect, p: (i32, i32)) -> Hit {
    let (px, py) = p;
    let near_left = (px - r.x).abs() <= HANDLE_HIT;
    let near_right = (px - r.right()).abs() <= HANDLE_HIT;
    let near_top = (py - r.y).abs() <= HANDLE_HIT;
    let near_bottom = (py - r.bottom()).abs() <= HANDLE_HIT;
    let within_x = r.x - HANDLE_HIT <= px && px <= r.right() + HANDLE_HIT;
    let within_y = r.y - HANDLE_HIT <= py && py <= r.bottom() + HANDLE_HIT;
    match (near_left || near_right, near_top || near_bottom) {
        (true, true) => Hit::Corner(match (near_left, near_top) {
            (true, true) => Corner::TopLeft,
            (false, true) => Corner::TopRight,
            (true, false) => Corner::BottomLeft,
            (false, false) => Corner::BottomRight,
        }),
        (true, false) if within_y => Hit::Edge(if near_left { Side::Left } else { Side::Right }),
        (false, true) if within_x => Hit::Edge(if near_top { Side::Top } else { Side::Bottom }),
        _ if r.contains(p) => Hit::Inside,
        _ => Hit::Outside,
    }
}

/// The drag a left press at `p` starts, given the selection on the same window.
fn press_kind(selection: Option<Rect>, p: (i32, i32)) -> DragKind {
    let Some(r) = selection else { return DragKind::Draw };
    let (left, right, top, bottom) = (r.x, r.right(), r.y, r.bottom());
    let resize = |fixed, moving, move_x, move_y| DragKind::Resize { fixed, moving, move_x, move_y };
    match hit_test(r, p) {
        Hit::Outside => DragKind::Draw,
        Hit::Inside => DragKind::Move { grab: (p.0 - left, p.1 - top) },
        Hit::Corner(Corner::TopLeft) => resize((right, bottom), (left, top), true, true),
        Hit::Corner(Corner::TopRight) => resize((left, bottom), (right, top), true, true),
        Hit::Corner(Corner::BottomLeft) => resize((right, top), (left, bottom), true, true),
        Hit::Corner(Corner::BottomRight) => resize((left, top), (right, bottom), true, true),
        Hit::Edge(Side::Left) => resize((right, bottom), (left, top), true, false),
        Hit::Edge(Side::Right) => resize((left, top), (right, bottom), true, false),
        Hit::Edge(Side::Top) => resize((right, bottom), (left, top), false, true),
        Hit::Edge(Side::Bottom) => resize((left, top), (right, bottom), false, true),
    }
}

/// `r` moved by `(dx, dy)`, kept inside a window of `(w, h)`.
fn nudge(r: Rect, dx: i32, dy: i32, (w, h): (i32, i32)) -> Rect {
    r.placed_within(r.x + dx, r.y + dy, w, h)
}

fn cursor_for(hit: Hit) -> CursorIcon {
    match hit {
        Hit::Outside => CursorIcon::Crosshair,
        Hit::Inside => CursorIcon::Move,
        Hit::Edge(Side::Left | Side::Right) => CursorIcon::EwResize,
        Hit::Edge(Side::Top | Side::Bottom) => CursorIcon::NsResize,
        Hit::Corner(Corner::TopLeft | Corner::BottomRight) => CursorIcon::NwseResize,
        Hit::Corner(Corner::TopRight | Corner::BottomLeft) => CursorIcon::NeswResize,
    }
}

/// The capture's pixels for one monitor (black where the monitor lies outside the capture).
fn monitor_pixels(capture: &Capture, monitor: Rect) -> Vec<u32> {
    let (w, h) = (monitor.w as usize, monitor.h as usize);
    let mut pixels = vec![0u32; w * h];
    let in_capture = capture.screen_to_capture(monitor);
    let Some(visible) = in_capture.intersect(&capture.rect()) else {
        warn!("monitor {monitor:?} is outside the capture");
        return pixels;
    };
    let dx = (visible.x - in_capture.x) as usize;
    for row in visible.y..visible.y + visible.h {
        let src = row as usize * capture.width as usize + visible.x as usize;
        let dst = (row - in_capture.y) as usize * w + dx;
        pixels[dst..dst + visible.w as usize]
            .copy_from_slice(&capture.pixels[src..src + visible.w as usize]);
    }
    pixels
}

/// Every channel × 115/256 (≈ 0.45); red and blue are done in one multiplication.
fn dim(p: u32) -> u32 {
    let rb = (((p & 0x00FF_00FF) * 115) >> 8) & 0x00FF_00FF;
    let g = (((p & 0x0000_FF00) * 115) >> 8) & 0x0000_FF00;
    rb | g
}

/// Above the selection; inside its top-left corner when there is no room above; never past
/// the right edge of a `window_w` wide window.
fn label_position(rect: Rect, (bw, bh): (i32, i32), window_w: i32) -> (i32, i32) {
    let (mut x, mut y) = (rect.x, rect.y - bh - LABEL_GAP);
    if y < 0 {
        (x, y) = (rect.x + LABEL_GAP, rect.y + LABEL_GAP);
    }
    (x.min(window_w - bw).max(0), y)
}

/// A border just outside `rect`, clipped to the `w`×`h` buffer.
fn draw_border(buffer: &mut [u32], w: i32, h: i32, rect: Rect) {
    let b = BORDER_WIDTH;
    let sides = [
        Rect::new(rect.x - b, rect.y - b, rect.w + 2 * b, b), // top
        Rect::new(rect.x - b, rect.y + rect.h, rect.w + 2 * b, b), // bottom
        Rect::new(rect.x - b, rect.y, b, rect.h),             // left
        Rect::new(rect.x + rect.w, rect.y, b, rect.h),        // right
    ];
    for side in sides {
        fill_rect(buffer, w, h, side, BORDER_COLOR);
    }
}

/// Squares centred on the four corners and the four edge midpoints of `rect`, clipped to the
/// `w`×`h` buffer.
fn draw_handles(buffer: &mut [u32], w: i32, h: i32, rect: Rect) {
    let xs = [rect.x, (rect.x + rect.right()) / 2, rect.right()];
    let ys = [rect.y, (rect.y + rect.bottom()) / 2, rect.bottom()];
    let half = HANDLE_SIZE / 2;
    for (j, &y) in ys.iter().enumerate() {
        for (i, &x) in xs.iter().enumerate() {
            // The centre of the selection is not a handle.
            if (i, j) != (1, 1) {
                fill_rect(buffer, w, h, Rect::new(x - half, y - half, HANDLE_SIZE, HANDLE_SIZE), BORDER_COLOR);
            }
        }
    }
}

fn fill_rect(buffer: &mut [u32], w: i32, h: i32, rect: Rect, color: u32) {
    let Some(r) = rect.intersect(&Rect::new(0, 0, w, h)) else { return };
    for row in r.y..r.y + r.h {
        let start = (row * w + r.x) as usize;
        buffer[start..start + r.w as usize].fill(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_matches_per_channel_math() {
        for p in [0x00FF_FFFF, 0x00E3_4133, 0x0001_0203, 0, 0x0080_7F01] {
            let expected = [16, 8, 0]
                .iter()
                .map(|&shift| ((((p >> shift) & 0xFF) * 115) >> 8) << shift)
                .fold(0, |acc, c| acc | c);
            assert_eq!(dim(p), expected, "pixel {p:06x}");
        }
    }

    #[test]
    fn label_goes_above_inside_or_left() {
        let size = (80, 24);
        // Room above: 4 px above the selection.
        assert_eq!(label_position(Rect::new(100, 200, 50, 50), size, 1920), (100, 172));
        // At the top edge: inside the selection.
        assert_eq!(label_position(Rect::new(100, 10, 50, 50), size, 1920), (104, 14));
        // Near the right edge: shifted left to stay on screen.
        assert_eq!(label_position(Rect::new(1900, 200, 20, 50), size, 1920), (1840, 172));
    }

    #[test]
    fn border_is_outside_and_clipped() {
        let (w, h) = (6, 5);
        let mut buffer = vec![0u32; (w * h) as usize];
        draw_border(&mut buffer, w, h, Rect::new(2, 2, 2, 2));
        let at = |x: i32, y: i32| buffer[(y * w + x) as usize];
        // Inside stays untouched; the 2 px ring around it is red; it is cut at the edges.
        assert_eq!(at(2, 2), 0);
        assert_eq!(at(3, 3), 0);
        assert_eq!(at(0, 0), BORDER_COLOR);
        assert_eq!(at(5, 4), BORDER_COLOR);
        assert_eq!(at(1, 3), BORDER_COLOR);
        assert_eq!(at(4, 2), BORDER_COLOR);
    }

    /// Left 100, right 299, top 100, bottom 199.
    const SEL: Rect = Rect { x: 100, y: 100, w: 200, h: 100 };
    const WINDOW: (i32, i32) = (1920, 1080);

    fn drag_to(selection: Rect, press: (i32, i32), cur: (i32, i32)) -> Rect {
        let drag = Drag { win: 0, kind: press_kind(Some(selection), press), start: press, cur };
        drag.rect(Some(selection), WINDOW)
    }

    #[test]
    fn hit_inside_edges_and_corners() {
        assert_eq!(hit_test(SEL, (200, 150)), Hit::Inside);
        assert_eq!(hit_test(SEL, (100, 150)), Hit::Edge(Side::Left));
        assert_eq!(hit_test(SEL, (299, 150)), Hit::Edge(Side::Right));
        assert_eq!(hit_test(SEL, (200, 100)), Hit::Edge(Side::Top));
        assert_eq!(hit_test(SEL, (200, 199)), Hit::Edge(Side::Bottom));
        assert_eq!(hit_test(SEL, (100, 100)), Hit::Corner(Corner::TopLeft));
        assert_eq!(hit_test(SEL, (299, 100)), Hit::Corner(Corner::TopRight));
        assert_eq!(hit_test(SEL, (100, 199)), Hit::Corner(Corner::BottomLeft));
        assert_eq!(hit_test(SEL, (299, 199)), Hit::Corner(Corner::BottomRight));
    }

    #[test]
    fn hit_reaches_8_px_outside_an_edge() {
        assert_eq!(hit_test(SEL, (92, 150)), Hit::Edge(Side::Left));
        assert_eq!(hit_test(SEL, (91, 150)), Hit::Outside);
        assert_eq!(hit_test(SEL, (307, 150)), Hit::Edge(Side::Right));
        assert_eq!(hit_test(SEL, (308, 150)), Hit::Outside);
        assert_eq!(hit_test(SEL, (200, 92)), Hit::Edge(Side::Top));
        assert_eq!(hit_test(SEL, (200, 91)), Hit::Outside);
        assert_eq!(hit_test(SEL, (200, 207)), Hit::Edge(Side::Bottom));
        assert_eq!(hit_test(SEL, (200, 208)), Hit::Outside);
        assert_eq!(hit_test(SEL, (92, 92)), Hit::Corner(Corner::TopLeft));
        assert_eq!(hit_test(SEL, (91, 92)), Hit::Outside);
    }

    #[test]
    fn tiny_selection_is_all_corner() {
        let r = Rect::new(10, 10, 3, 3);
        for y in 10..13 {
            for x in 10..13 {
                assert_eq!(hit_test(r, (x, y)), Hit::Corner(Corner::TopLeft), "at ({x}, {y})");
            }
        }
    }

    #[test]
    fn press_outside_or_without_selection_draws() {
        assert_eq!(press_kind(None, (200, 150)), DragKind::Draw);
        assert_eq!(press_kind(Some(SEL), (50, 50)), DragKind::Draw);
        assert_eq!(press_kind(Some(SEL), (130, 160)), DragKind::Move { grab: (30, 60) });
    }

    #[test]
    fn resize_past_the_fixed_corner_flips() {
        // Bottom-right corner dragged up-left past the top-left one.
        let r = drag_to(SEL, (299, 199), (50, 40));
        assert_eq!(r, Rect::new(50, 40, 51, 61));
        assert_eq!((r.right(), r.bottom()), (100, 100));
    }

    #[test]
    fn right_edge_drag_keeps_y_and_height() {
        for cur in [(400, 10), (20, 1000), (299, 150), (100, 199)] {
            let r = drag_to(SEL, (299, 150), cur);
            assert_eq!((r.y, r.h), (SEL.y, SEL.h), "cursor at {cur:?}");
        }
        assert_eq!(drag_to(SEL, (299, 150), (400, 10)), Rect::new(100, 100, 301, 100));
        assert_eq!(drag_to(SEL, (299, 150), (20, 10)), Rect::new(20, 100, 81, 100));
    }

    #[test]
    fn top_left_corner_drag_moves_both_axes() {
        assert_eq!(drag_to(SEL, (100, 100), (150, 120)), Rect::new(150, 120, 150, 80));
    }

    #[test]
    fn move_is_clamped_to_the_window() {
        // Grabbed 100 px right of and 50 px below the top-left corner.
        let grab = (200, 150);
        assert_eq!(drag_to(SEL, grab, (250, 170)), Rect::new(150, 120, 200, 100));
        // Left and top.
        assert_eq!(drag_to(SEL, grab, (5, 5)), Rect::new(0, 0, 200, 100));
        // Right and bottom.
        assert_eq!(drag_to(SEL, grab, (1919, 1079)), Rect::new(1720, 980, 200, 100));
        // One axis at a time.
        assert_eq!(drag_to(SEL, grab, (1919, 170)), Rect::new(1720, 120, 200, 100));
        assert_eq!(drag_to(SEL, grab, (250, 0)), Rect::new(150, 0, 200, 100));
    }

    #[test]
    fn nudge_is_clamped_to_the_window() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!(nudge(r, 1, 0, WINDOW), Rect::new(11, 20, 30, 40));
        assert_eq!(nudge(r, 0, -10, WINDOW), Rect::new(10, 10, 30, 40));
        assert_eq!(nudge(r, -20, -30, WINDOW), Rect::new(0, 0, 30, 40));
        let corner = Rect::new(1885, 1035, 30, 40);
        assert_eq!(nudge(corner, 10, 10, WINDOW), Rect::new(1890, 1040, 30, 40));
    }

    #[test]
    fn handles_sit_on_corners_and_midpoints() {
        let (w, h) = (40, 40);
        let mut buffer = vec![0u32; (w * h) as usize];
        // Corners at 4 and 23, midpoints at 13: handles cover 0..=7, 9..=16 and 19..=26.
        draw_handles(&mut buffer, w, h, Rect::new(4, 4, 20, 20));
        let at = |x: i32, y: i32| buffer[(y * w + x) as usize];
        assert_eq!(at(0, 0), BORDER_COLOR);
        assert_eq!(at(7, 7), BORDER_COLOR);
        assert_eq!(at(8, 4), 0);
        assert_eq!(at(9, 0), BORDER_COLOR);
        assert_eq!(at(16, 7), BORDER_COLOR);
        assert_eq!(at(17, 4), 0);
        assert_eq!(at(0, 9), BORDER_COLOR);
        assert_eq!(at(0, 8), 0);
        assert_eq!(at(26, 26), BORDER_COLOR);
        assert_eq!(at(27, 27), 0);
        // No handle in the middle.
        assert_eq!(at(13, 13), 0);
    }

    #[test]
    fn handles_are_clipped_to_the_window() {
        let (w, h) = (10, 10);
        let mut buffer = vec![0u32; (w * h) as usize];
        draw_handles(&mut buffer, w, h, Rect::new(0, 0, 10, 10));
        assert_eq!(buffer[0], BORDER_COLOR);
        assert_eq!(buffer[(h * w - 1) as usize], BORDER_COLOR);
    }

    #[test]
    fn draw_spans_press_and_cursor() {
        let drag = Drag { win: 0, kind: DragKind::Draw, start: (40, 30), cur: (10, 20) };
        assert_eq!(drag.rect(None, WINDOW), Rect::new(10, 20, 31, 11));
    }

    #[test]
    fn monitor_pixels_come_from_the_right_place() {
        // Capture of two 2x1 monitors side by side, starting at x = -2.
        let capture = Capture { x0: -2, y0: 0, width: 4, height: 1, pixels: vec![1, 2, 3, 4] };
        assert_eq!(monitor_pixels(&capture, Rect::new(-2, 0, 2, 1)), [1, 2]);
        assert_eq!(monitor_pixels(&capture, Rect::new(0, 0, 2, 1)), [3, 4]);
        // Partly outside: the missing pixels stay black.
        assert_eq!(monitor_pixels(&capture, Rect::new(1, 0, 2, 1)), [4, 0]);
    }
}
