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
use crate::text::Text;
use crate::util::{self, Rect};

const BORDER_COLOR: u32 = 0x00E3_4133; // ScreenCloud red (227, 65, 51)
const BORDER_WIDTH: i32 = 2;
const HELP: &str =
    "Drag to select  ·  Enter: copy + save  ·  Ctrl+S: save as  ·  Ctrl+C: copy  ·  Esc: cancel";
const HELP_TOP: i32 = 40;
/// Gap between the size label and the selection below it.
const LABEL_GAP: i32 = 4;

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
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    win: usize,
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
            info!("the overlay did not get the keyboard focus; the first click will focus it");
        }
        info!("region overlay open on {} monitor(s)", windows.len());

        Ok(OverlaySession {
            capture,
            text,
            windows,
            drag: None,
            selection: None,
            ctrl: false,
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
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let at = self.windows[idx].last_cursor;
                self.selection = None;
                self.drag = Some(Drag { win: idx, start: at, cur: at });
                self.started = true;
                // Also clears a selection on another monitor and the help line everywhere.
                self.redraw_all();
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                if let Some(drag) = self.drag.filter(|d| d.win == idx) {
                    self.drag = None;
                    self.selection = Some(Selection { win: idx, rect: Rect::from_corners(drag.start, drag.cur) });
                    self.windows[idx].window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                return Outcome::Cancel;
            }
            WindowEvent::ModifiersChanged(modifiers) => self.ctrl = modifiers.state().control_key(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
                // Physical keys: with the Ukrainian layout the logical key for C is "с".
                match event.physical_key {
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
            WindowEvent::Resized(size) => {
                let m = self.windows[idx].monitor;
                if (size.width as i32, size.height as i32) != (m.w, m.h) {
                    warn!("overlay for {m:?} was resized to {}x{}", size.width, size.height);
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

    /// The rectangle to show on window `idx`: the one being dragged, else the selection.
    fn live_rect(&self, idx: usize) -> Option<Rect> {
        match (self.drag, self.selection) {
            (Some(d), _) if d.win == idx => Some(Rect::from_corners(d.start, d.cur)),
            (None, Some(s)) if s.win == idx => Some(s.rect),
            _ => None,
        }
    }

    fn render(&mut self, idx: usize) {
        let live = self.live_rect(idx);
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
            .with_corner_preference(CornerPreference::DoNotRound);
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

        Ok(OverlayWindow { window, _context: context, surface, monitor, base, dimmed, last_cursor })
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
