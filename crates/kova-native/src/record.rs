//! Scripted recordings of a UI as animated GIFs (feature `record`).
//!
//! A [`Recorder`] drives a [`Headless`] tree with a simulated clock and the
//! real GPU renderer. Pointer movement is interpolated so hover effects
//! animate, a cursor and click ripples are drawn on top, and the frames are
//! encoded as a GIF that only stores the pixels that changed:
//!
//! ```no_run
//! use kova_native::prelude::*;
//! use kova_native::record::Recorder;
//!
//! # fn main() -> KovaResult<()> {
//! let open = signal(false);
//! let ui = Headless::new(Size::new(640.0, 400.0), move || {
//!     column().center().size_full()
//!         .child(button("Open").id("open").on_click(move |_| open.set(true)))
//!         .child(dialog(open).title("Hello").content(|| text("Recorded with Kova")))
//! });
//! let mut rec = Recorder::new(ui, 1.0)?;
//! rec.hold(400.ms());
//! rec.click_id("open");
//! rec.hold(1200.ms());
//! rec.save_gif("dialog.gif")?;
//! # Ok(())
//! # }
//! ```

use kova_native_core::{Duration, ElementId, KovaError, KovaResult, Point, Size};
use kova_native_input::MouseButton;
use kova_native_widgets::headless::Headless;
use std::path::Path;

/// Recording frame rate.
pub const FPS: u32 = 25;

struct Ripple {
    at: Point,
    age: Duration,
}

struct Frame {
    rgba: Vec<u8>,
    /// Display time in centiseconds.
    delay: u16,
}

/// Records a scripted interaction. See the module docs.
pub struct Recorder {
    ui: Headless,
    frames: Vec<Frame>,
    width: u32,
    height: u32,
    scale: f32,
    pointer: Option<Point>,
    ripples: Vec<Ripple>,
    /// Fractional centiseconds carried between frames.
    carry: f32,
}

impl Recorder {
    /// Switches `ui` to GPU rendering at `scale` and captures the first frame.
    pub fn new(ui: Headless, scale: f32) -> KovaResult<Self> {
        let mut ui = ui.with_gpu()?;
        let size = ui.size();
        ui.resize(size, scale);
        let (width, height) = ui.pixel_size();
        if width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err(KovaError::Other("recording too large for GIF".into()));
        }
        Ok(Recorder {
            ui,
            frames: Vec::new(),
            width,
            height,
            scale,
            pointer: None,
            ripples: Vec::new(),
            carry: 0.0,
        })
    }

    /// The driven UI (for direct input or assertions).
    pub fn ui(&mut self) -> &mut Headless {
        &mut self.ui
    }

    /// Number of frames captured so far.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Total recorded time.
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.frames.iter().map(|f| f.delay as u64 * 10).sum())
    }

    fn capture(&mut self) -> KovaResult<()> {
        let mut rgba = self.ui.capture()?;
        let dt = Duration::from_secs_f32(1.0 / FPS as f32);
        for ripple in &mut self.ripples {
            draw_ripple(&mut rgba, self.width, self.height, ripple, self.scale);
            ripple.age += dt;
        }
        self.ripples.retain(|r| r.age < Duration::from_millis(420));
        if let Some(p) = self.pointer {
            draw_cursor(&mut rgba, self.width, self.height, p, self.scale);
        }
        let exact = 100.0 / FPS as f32 + self.carry;
        let delay = exact.floor();
        self.carry = exact - delay;
        match self.frames.last_mut() {
            // Identical frames only extend the previous frame's delay.
            Some(last) if last.rgba == rgba => last.delay += delay as u16,
            _ => self.frames.push(Frame {
                rgba,
                delay: delay as u16,
            }),
        }
        Ok(())
    }

    /// Records `duration` of the UI running (animations, timers, tasks).
    pub fn hold(&mut self, duration: Duration) -> KovaResult<()> {
        let step = Duration::from_secs_f32(1.0 / FPS as f32);
        let frames = (duration.as_secs_f32() * FPS as f32).round().max(1.0) as u32;
        for _ in 0..frames {
            self.ui.advance(step);
            self.capture()?;
        }
        Ok(())
    }

    /// Runs an action on the UI (input, signal writes) and captures a frame.
    pub fn act(&mut self, f: impl FnOnce(&mut Headless)) -> KovaResult<()> {
        f(&mut self.ui);
        self.capture()
    }

    /// Glides the pointer to `target` over `duration`, dispatching moves.
    pub fn move_to(&mut self, target: Point, duration: Duration) -> KovaResult<()> {
        let from = self.pointer.unwrap_or(Point::new(
            self.ui.size().width * 0.5,
            self.ui.size().height + 20.0,
        ));
        let step = Duration::from_secs_f32(1.0 / FPS as f32);
        let frames = (duration.as_secs_f32() * FPS as f32).round().max(1.0) as u32;
        for i in 1..=frames {
            let t = i as f32 / frames as f32;
            // Ease in-out for a natural glide.
            let e = if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            };
            let p = Point::new(
                from.x + (target.x - from.x) * e,
                from.y + (target.y - from.y) * e,
            );
            self.pointer = Some(p);
            self.ui.mouse_move(p);
            self.ui.advance(step);
            self.capture()?;
        }
        Ok(())
    }

    /// Clicks at the current pointer position with a ripple.
    pub fn click(&mut self) -> KovaResult<()> {
        let p = self.pointer.unwrap_or_default();
        self.ripples.push(Ripple {
            at: p,
            age: Duration::ZERO,
        });
        self.ui.mouse_down(p, MouseButton::Left);
        self.capture()?;
        self.ui.mouse_up(p, MouseButton::Left);
        self.capture()
    }

    /// Glides to the element with `id` and clicks it.
    pub fn click_id(&mut self, id: impl Into<ElementId>) -> KovaResult<()> {
        let id = id.into();
        let b = self
            .ui
            .bounds_of(id.clone())
            .ok_or_else(|| KovaError::Other(format!("no painted element {id:?}")))?;
        self.move_to(b.center(), Duration::from_millis(450))?;
        self.click()
    }

    /// Glides to the text element showing `content` and clicks it.
    pub fn click_text(&mut self, content: &str) -> KovaResult<()> {
        let node = self
            .ui
            .find_text(content)
            .ok_or_else(|| KovaError::Other(format!("no painted text {content:?}")))?;
        let b = self.ui.tree().visual_bounds(node).expect("painted");
        self.move_to(b.center(), Duration::from_millis(450))?;
        self.click()
    }

    /// Glides to the element with `id` without clicking (hover).
    pub fn hover_id(&mut self, id: impl Into<ElementId>) -> KovaResult<()> {
        let id = id.into();
        let b = self
            .ui
            .bounds_of(id.clone())
            .ok_or_else(|| KovaError::Other(format!("no painted element {id:?}")))?;
        self.move_to(b.center(), Duration::from_millis(450))
    }

    /// Types text into the focused element at a human pace.
    pub fn type_text(&mut self, text: &str) -> KovaResult<()> {
        for c in text.chars() {
            self.ui.type_text(&c.to_string());
            self.hold(Duration::from_millis(70))?;
        }
        Ok(())
    }

    /// Presses a keystroke (`"enter"`, `"down"`, `"ctrl-k"`...).
    pub fn press(&mut self, keystroke: &str) -> KovaResult<()> {
        self.ui.press(keystroke);
        self.capture()
    }

    /// Encodes the frames as an infinitely looping GIF.
    pub fn save_gif(&self, path: impl AsRef<Path>) -> KovaResult<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let (w, h) = (self.width as u16, self.height as u16);
        let gif_err = |e: gif::EncodingError| KovaError::Other(format!("GIF encoding: {e}"));
        let mut encoder = gif::Encoder::new(file, w, h, &[]).map_err(gif_err)?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(gif_err)?;
        let mut previous: Option<&[u8]> = None;
        for frame in &self.frames {
            let rect = match previous {
                Some(prev) => changed_rect(prev, &frame.rgba, self.width, self.height),
                None => Some((0, 0, self.width, self.height)),
            };
            // Unchanged frames were merged while capturing.
            let (x, y, rw, rh) = rect.unwrap_or((0, 0, 1, 1));
            let mut crop = crop_rgba(&frame.rgba, self.width, x, y, rw, rh);
            let mut out = gif::Frame::from_rgba_speed(rw as u16, rh as u16, &mut crop, 10);
            out.left = x as u16;
            out.top = y as u16;
            out.delay = frame.delay.max(2);
            out.dispose = gif::DisposalMethod::Keep;
            encoder.write_frame(&out).map_err(gif_err)?;
            previous = Some(&frame.rgba);
        }
        Ok(())
    }

    /// Pixel size of the recording.
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Logical size of the recorded UI.
    pub fn size(&self) -> Size {
        self.ui.size()
    }
}

/// Bounding box `(x, y, w, h)` of pixels that differ, or `None`.
fn changed_rect(a: &[u8], b: &[u8], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let stride = width as usize * 4;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..height as usize {
        let (ra, rb) = (&a[y * stride..][..stride], &b[y * stride..][..stride]);
        if ra == rb {
            continue;
        }
        let (pa, pb) = (ra.as_chunks::<4>().0, rb.as_chunks::<4>().0);
        let first = pa.iter().zip(pb).position(|(p, q)| p != q).unwrap_or(0) as u32;
        let last = pa.iter().zip(pb).rposition(|(p, q)| p != q).unwrap_or(0) as u32;
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y as u32);
        y1 = y1.max(y as u32);
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

fn crop_rgba(rgba: &[u8], width: u32, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * width + x) * 4) as usize;
        out.extend_from_slice(&rgba[start..start + (w * 4) as usize]);
    }
    out
}

/// Arrow cursor: `X` outline, `.` fill.
const CURSOR: [&str; 17] = [
    "X",
    "XX",
    "X.X",
    "X..X",
    "X...X",
    "X....X",
    "X.....X",
    "X......X",
    "X.......X",
    "X........X",
    "X.....XXXXX",
    "X..X..X",
    "X.X X..X",
    "XX  X..X",
    "X    X..X",
    "     X..X",
    "      XX",
];

fn blend(rgba: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 3], alpha: f32) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height || alpha <= 0.0 {
        return;
    }
    let i = ((y as u32 * width + x as u32) * 4) as usize;
    for c in 0..3 {
        let dst = rgba[i + c] as f32;
        rgba[i + c] = (dst + (color[c] as f32 - dst) * alpha.min(1.0)).round() as u8;
    }
}

fn draw_cursor(rgba: &mut [u8], width: u32, height: u32, at: Point, scale: f32) {
    let px = scale.max(1.0).round() as i32;
    let (ox, oy) = ((at.x * scale).round() as i32, (at.y * scale).round() as i32);
    for (row, line) in CURSOR.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let color = match ch {
                'X' => [16, 16, 20],
                '.' => [250, 250, 252],
                _ => continue,
            };
            for dy in 0..px {
                for dx in 0..px {
                    blend(
                        rgba,
                        width,
                        height,
                        ox + col as i32 * px + dx,
                        oy + row as i32 * px + dy,
                        color,
                        1.0,
                    );
                }
            }
        }
    }
}

fn draw_ripple(rgba: &mut [u8], width: u32, height: u32, ripple: &Ripple, scale: f32) {
    let t = ripple.age.as_secs_f32() / 0.42;
    let radius = (6.0 + 18.0 * t) * scale;
    let alpha = 0.55 * (1.0 - t);
    let thickness = 2.0 * scale;
    let (cx, cy) = (ripple.at.x * scale, ripple.at.y * scale);
    let r = (radius + thickness).ceil() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let coverage = 1.0 - ((d - radius).abs() - thickness * 0.5).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend(
                    rgba,
                    width,
                    height,
                    cx as i32 + dx,
                    cy as i32 + dy,
                    [124, 92, 255],
                    alpha * coverage,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_rect_finds_the_bounding_box() {
        let (w, h) = (6u32, 4u32);
        let a = vec![0u8; (w * h * 4) as usize];
        let mut b = a.clone();
        assert_eq!(changed_rect(&a, &b, w, h), None);
        b[((w + 2) * 4) as usize] = 255;
        b[((2 * w + 4) * 4 + 1) as usize] = 9;
        assert_eq!(changed_rect(&a, &b, w, h), Some((2, 1, 3, 2)));
        let crop = crop_rgba(&b, w, 2, 1, 3, 2);
        assert_eq!(crop.len(), 3 * 2 * 4);
        assert_eq!(crop[0], 255);
    }
}
