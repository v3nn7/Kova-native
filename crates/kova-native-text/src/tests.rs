use super::*;
use std::sync::Mutex;
use std::sync::OnceLock;

// Loading system fonts is slow; share one TextSystem across tests.
fn with_ts<R>(f: impl FnOnce(&mut TextSystem) -> R) -> R {
    static TS: OnceLock<Mutex<SendTs>> = OnceLock::new();
    struct SendTs(TextSystem);
    // SAFETY: tests access the system under a mutex, one thread at a time.
    unsafe impl Send for SendTs {}
    let m = TS.get_or_init(|| Mutex::new(SendTs(TextSystem::new())));
    let mut guard = m.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard.0)
}

fn style(size: f32) -> TextStyle {
    TextStyle {
        size,
        ..Default::default()
    }
}

#[test]
fn system_has_fonts() {
    with_ts(|ts| {
        assert!(ts.face_count() > 0, "no system fonts found");
        assert!(!ts.ui_family().is_empty());
    });
}

#[test]
fn measure_and_wrap() {
    with_ts(|ts| {
        let mut layout = TextLayout::new();
        assert!(layout.set(
            ts,
            "Hello Kova Native, a GPU accelerated GUI framework",
            &style(16.0)
        ));
        let single = layout.measure(ts, None);
        assert!(single.width > 100.0, "{single:?}");
        let line_height = 16.0 * 1.4;
        assert!((single.height - line_height).abs() < 0.5, "{single:?}");

        let wrapped = layout.measure(ts, Some(single.width / 2.0));
        assert!(wrapped.height >= line_height * 2.0 - 0.5, "{wrapped:?}");
        assert!(wrapped.width <= single.width / 2.0 + 0.5);

        // Paint-only change does not invalidate layout.
        let mut red = style(16.0);
        red.color = kova_native_core::rgb(0xff0000);
        assert!(!layout.set(
            ts,
            "Hello Kova Native, a GPU accelerated GUI framework",
            &red
        ));
        // Bigger font does.
        assert!(layout.set(
            ts,
            "Hello Kova Native, a GPU accelerated GUI framework",
            &style(20.0)
        ));
    });
}

#[test]
fn glyphs_rasterize_and_scale() {
    with_ts(|ts| {
        let mut layout = TextLayout::new();
        layout.set(ts, "Ag", &style(20.0));
        layout.layout(ts, None);
        let mut glyphs = Vec::new();
        layout.for_each_glyph(kova_native_core::point(10.0, 10.0), 2.0, |g| glyphs.push(g));
        assert_eq!(glyphs.len(), 2);
        let raster = ts.rasterize(glyphs[0].key).expect("glyph bitmap");
        // At 2x the 'A' must be roughly 2 * 0.7 * 20px tall.
        assert!(raster.height > 20, "{} px", raster.height);
        assert_eq!(raster.data.len(), (raster.width * raster.height) as usize);
        assert!(glyphs[0].x >= 20);
    });
}

#[test]
fn hit_testing_and_carets() {
    with_ts(|ts| {
        let mut layout = TextLayout::new();
        layout.set(ts, "abc\ndef", &style(16.0));
        layout.layout(ts, None);
        assert_eq!(layout.lines().len(), 2);
        assert_eq!(layout.hit_test(kova_native_core::point(-5.0, 2.0)), 0);
        let second_line = layout.hit_test(kova_native_core::point(0.0, 16.0 * 1.4 + 2.0));
        assert_eq!(second_line, 4);
        let end = layout.hit_test(kova_native_core::point(1000.0, 30.0));
        assert_eq!(end, 7);
        let c0 = layout.caret_bounds(0);
        let c2 = layout.caret_bounds(2);
        assert!(c2.origin.x > c0.origin.x);
        let c4 = layout.caret_bounds(4);
        assert!(c4.origin.y > c0.origin.y);
        let sel = layout.selection_bounds(1, 6);
        assert_eq!(sel.len(), 2);
    });
}

#[test]
fn unicode_fallback_shapes() {
    with_ts(|ts| {
        let mut layout = TextLayout::new();
        layout.set(ts, "Zażółć gęślą jaźń — 日本語 ✓", &style(16.0));
        let size = layout.measure(ts, None);
        assert!(size.width > 50.0);
    });
}
