use super::*;
use kova_native_core::{Edges, Size};

fn row(gap: f32) -> LayoutStyle {
    LayoutStyle {
        direction: FlexDirection::Row,
        gap: Axes::both(px(gap)),
        ..Default::default()
    }
}

fn fixed(w: f32, h: f32) -> LayoutStyle {
    LayoutStyle {
        size: Axes {
            width: px(w),
            height: px(h),
        },
        ..Default::default()
    }
}

#[test]
fn row_with_gap_and_padding() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    let a = e.create(&fixed(50.0, 20.0), None);
    let b = e.create(&fixed(30.0, 40.0), None);
    let mut style = row(10.0);
    style.padding = Edges::all(px(5.0));
    let root = e.create(&style, None);
    e.set_children(root, &[a, b]);
    e.compute(root, Size::new(500.0, 500.0), |_, _| Size::ZERO);

    let la = e.layout(a);
    let lb = e.layout(b);
    assert_eq!(la.location, kova_native_core::point(5.0, 5.0));
    assert_eq!(lb.location.x, 5.0 + 50.0 + 10.0);
    // An auto-sized root shrink-wraps its content, like CSS.
    assert_eq!(e.layout(root).size.width, 5.0 + 50.0 + 10.0 + 30.0 + 5.0);
}

#[test]
fn column_flex_grow_and_percent() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    let header = e.create(&fixed(100.0, 30.0), None);
    let body = e.create(
        &LayoutStyle {
            flex_grow: 1.0,
            ..Default::default()
        },
        None,
    );
    let half = e.create(
        &LayoutStyle {
            size: Axes {
                width: pct(50.0),
                height: px(10.0),
            },
            ..Default::default()
        },
        None,
    );
    let root = e.create(
        &LayoutStyle {
            direction: FlexDirection::Column,
            size: Axes {
                width: px(200.0),
                height: px(300.0),
            },
            ..Default::default()
        },
        None,
    );
    e.set_children(root, &[header, body, half]);
    e.compute(root, Size::new(200.0, 300.0), |_, _| Size::ZERO);
    assert_eq!(e.layout(body).size.height, 300.0 - 30.0 - 10.0);
    assert_eq!(e.layout(half).size.width, 100.0);
}

#[test]
fn measured_leaf_and_incremental_dirtying() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    let text = e.create(&LayoutStyle::default(), Some(7));
    let root = e.create(
        &LayoutStyle {
            align_items: Some(Align::Start),
            ..row(0.0)
        },
        None,
    );
    e.set_children(root, &[text]);
    let mut width = 80.0;
    let mut calls = 0;
    e.compute(root, Size::new(400.0, 400.0), |ctx, _| {
        assert_eq!(ctx, 7);
        calls += 1;
        Size::new(width, 16.0)
    });
    assert_eq!(e.layout(text).size, Size::new(80.0, 16.0));
    assert!(calls > 0);

    // Unchanged tree: taffy serves cached results without measuring.
    let mut calls_again = 0;
    e.compute(root, Size::new(400.0, 400.0), |_, _| {
        calls_again += 1;
        Size::new(width, 16.0)
    });
    assert_eq!(calls_again, 0);

    width = 120.0;
    e.mark_dirty(text);
    e.compute(root, Size::new(400.0, 400.0), |_, _| Size::new(width, 16.0));
    assert_eq!(e.layout(text).size.width, 120.0);
}

#[test]
fn absolute_positioning() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    let badge = e.create(
        &LayoutStyle {
            position: Position::Absolute,
            inset: Edges {
                top: px(4.0),
                right: px(4.0),
                bottom: Length::Auto,
                left: Length::Auto,
            },
            ..fixed(10.0, 10.0)
        },
        None,
    );
    let root = e.create(&fixed(100.0, 50.0), None);
    e.set_children(root, &[badge]);
    e.compute(root, Size::new(100.0, 50.0), |_, _| Size::ZERO);
    assert_eq!(e.layout(badge).location, kova_native_core::point(86.0, 4.0));
}

#[test]
fn grid_columns_and_span() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    let mut root_style = LayoutStyle {
        display: Display::Grid,
        size: Axes {
            width: px(300.0),
            height: Length::Auto,
        },
        ..Default::default()
    };
    root_style.grid_mut().template_columns = vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)];
    let mut wide = fixed(0.0, 20.0);
    wide.size.width = Length::Auto;
    wide.grid_mut().column = GridPlacement::Span(2);
    let a = e.create(&wide, None);
    let b = e.create(
        &LayoutStyle {
            size: Axes {
                width: Length::Auto,
                height: px(20.0),
            },
            ..Default::default()
        },
        None,
    );
    let root = e.create(&root_style, None);
    e.set_children(root, &[a, b]);
    e.compute(root, Size::new(300.0, 300.0), |_, _| Size::ZERO);
    assert_eq!(e.layout(a).size.width, 200.0);
    assert_eq!(e.layout(b).location.x, 200.0);
}

#[test]
fn scroll_content_size() {
    let mut e: LayoutEngine<u32> = LayoutEngine::new();
    // As in CSS, flex items of a scroll container need `flex-shrink: 0` to
    // keep their size (the element tree applies this automatically).
    let tall = e.create(
        &LayoutStyle {
            flex_shrink: 0.0,
            ..fixed(50.0, 400.0)
        },
        None,
    );
    let root = e.create(
        &LayoutStyle {
            overflow_y: Overflow::Scroll,
            direction: FlexDirection::Column,
            ..fixed(100.0, 100.0)
        },
        None,
    );
    e.set_children(root, &[tall]);
    e.compute(root, Size::new(100.0, 100.0), |_, _| Size::ZERO);
    let l = e.layout(root);
    assert_eq!(l.size.height, 100.0);
    assert_eq!(l.content_size.height, 400.0);
    // The child must not be shrunk to fit a scroll container.
    assert_eq!(e.layout(tall).size.height, 400.0);
}
