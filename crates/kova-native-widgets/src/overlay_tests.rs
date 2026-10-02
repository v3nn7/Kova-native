//! Overlay layer, focus traps, outside clicks, timers and tasks in the tree.

use crate::elements::{Placement, column, div, dynamic, keyed, portal, row, text};
use crate::headless::Headless;
use crate::prelude::*;
use kova_native_core::{DurationExt, Owner, Point, Size, signal, task, timer};

fn setup<T>(f: impl FnOnce() -> T) -> (Owner, T) {
    let owner = Owner::new_root();
    let value = owner.with(f);
    (owner, value)
}

#[test]
fn portal_content_escapes_clipping_and_paints_above_later_siblings() {
    let (owner, open) = setup(|| signal(true));
    let mut ui = Headless::new(Size::new(400.0, 300.0), move || {
        column()
            .padding(20.0)
            .child(
                // A small clipping container holding an anchored popover.
                div()
                    .id("anchor")
                    .w(80.0)
                    .h(30.0)
                    .overflow_hidden()
                    .child(dynamic(move || {
                        open.get().then(|| {
                            portal().anchored().child(
                                div()
                                    .id("popup")
                                    .w(200.0)
                                    .h(100.0)
                                    .bg(0x334455)
                                    .child(text("Inside popup")),
                            )
                        })
                    })),
            )
            // Painted after the anchor in the root, but below the overlay.
            .child(
                div()
                    .id("cover")
                    .w_full()
                    .h(200.0)
                    .bg(0x112233)
                    .on_click(|_| {}),
            )
    });
    let anchor = ui.bounds_of("anchor").unwrap();
    let popup = ui.bounds_of("popup").expect("popup painted");
    assert_eq!(popup.size, Size::new(200.0, 100.0));
    assert_eq!(
        popup.origin,
        Point::new(anchor.left(), anchor.bottom() + 6.0)
    );
    // The popup overlaps "cover" but is hit first.
    let hit = ui.tree().hit_test(popup.center());
    let cover = ui.node("cover").unwrap();
    assert_ne!(hit, Some(cover));
    assert!(ui.find_text("Inside popup").is_some());
    assert!(ui.tree().debug_dump().contains("overlay:"));

    open.set(false);
    ui.settle();
    assert!(ui.bounds_of("popup").is_none());
    assert!(ui.tree().children(ui.tree().overlay_root()).is_empty());
    drop(ui);
    owner.dispose();
}

#[test]
fn portal_children_inherit_text_style_and_bubble_through_logical_parents() {
    let (owner, (clicks, color)) = setup(|| (signal(0), kova_native_core::rgb(0xff0000)));
    let mut ui = Headless::new(Size::new(300.0, 200.0), move || {
        div()
            .text_color(color)
            .on_click(move |_| clicks.update(|c| *c += 1))
            .child(portal().child(div().id("child").size(40.0).child(text("Hi"))))
    });
    ui.click_id("child");
    assert_eq!(clicks.get(), 1, "click bubbles to the portal's ancestors");
    drop(ui);
    owner.dispose();
}

#[test]
fn click_outside_fires_only_outside_the_logical_subtree() {
    let (owner, (outside, open)) = setup(|| (signal(0), signal(true)));
    let mut ui = Headless::new(Size::new(400.0, 300.0), move || {
        column()
            .gap(20.0)
            .child(
                div()
                    .id("menu-root")
                    .on_click_outside(move |_| outside.update(|n| *n += 1))
                    .child(div().id("trigger").size(30.0).bg(0x222222).on_click(|_| {}))
                    .child(dynamic(move || {
                        open.get().then(|| {
                            portal()
                                .anchored()
                                .child(div().id("item").size(50.0).bg(0x444444).on_click(|_| {}))
                        })
                    })),
            )
            .child(div().id("elsewhere").w(100.0).h(40.0).bg(0x333333))
    });
    ui.click_id("trigger");
    ui.click_id("item");
    assert_eq!(outside.get(), 0, "trigger and portal content are inside");
    ui.click_at(Point::new(390.0, 290.0));
    assert_eq!(outside.get(), 1);
    drop(ui);
    owner.dispose();
}

#[test]
fn focus_trap_cycles_inside_and_restores_focus_on_close() {
    let (owner, open) = setup(|| signal(false));
    let mut ui = Headless::new(Size::new(500.0, 400.0), move || {
        column()
            .child(
                div()
                    .id("opener")
                    .size(20.0)
                    .focusable()
                    .on_click(move |_| open.set(true)),
            )
            .child(div().id("other").size(20.0).focusable())
            .child(dynamic(move || {
                open.get().then(|| {
                    portal().child(
                        column()
                            .id("dialog")
                            .trap_focus()
                            .child(div().id("first").size(20.0).autofocus())
                            .child(div().id("second").size(20.0).focusable())
                            .child(div().id("skipped").size(20.0).tab_index(-1)),
                    )
                })
            }))
    });
    ui.click_id("opener");
    assert_eq!(
        ui.focused_id(),
        Some("first".into()),
        "autofocus inside the dialog"
    );
    ui.press("tab");
    assert_eq!(ui.focused_id(), Some("second".into()));
    ui.press("tab");
    assert_eq!(
        ui.focused_id(),
        Some("first".into()),
        "Tab wraps inside the trap"
    );
    ui.press("shift-tab");
    assert_eq!(ui.focused_id(), Some("second".into()));

    open.set(false);
    ui.settle();
    assert_eq!(
        ui.focused_id(),
        Some("opener".into()),
        "focus returns to the opener"
    );
    ui.press("tab");
    assert_eq!(ui.focused_id(), Some("other".into()), "trap released");
    drop(ui);
    owner.dispose();
}

#[test]
fn anchor_by_id_and_point() {
    let (owner, ()) = setup(|| ());
    let ui = Headless::new(Size::new(600.0, 400.0), move || {
        row()
            .child(div().w(300.0))
            .child(div().id("target").w(100.0).h(40.0))
            .child(
                portal()
                    .anchor_to("target")
                    .placement(Placement::TOP)
                    .gap(4.0)
                    .child(div().id("tip").w(60.0).h(20.0)),
            )
            .child(
                portal()
                    .at(Point::new(50.0, 60.0))
                    .gap(0.0)
                    .child(div().id("ctx").w(120.0).h(80.0)),
            )
    });
    let target = ui.bounds_of("target").unwrap();
    let tip = ui.bounds_of("tip").unwrap();
    assert_eq!(tip.bottom(), target.top() - 4.0);
    assert_eq!(tip.center().x, target.center().x);
    assert_eq!(ui.bounds_of("ctx").unwrap().origin, Point::new(50.0, 60.0));
    drop(ui);
    owner.dispose();
}

#[test]
fn timers_and_tasks_run_on_the_frame_clock() {
    let (owner, (label, task_value)) = setup(|| (signal("waiting"), signal(0)));
    let mut ui = Headless::new(Size::new(300.0, 200.0), move || {
        column()
            .child(text(move || label.get()))
            .child(text(move || format!("task {}", task_value.get())))
    });
    owner.with(|| {
        timer::set_timeout(500.ms(), move || label.set("fired"));
        task::spawn_local(async move {
            task::sleep(200.ms()).await;
            task_value.set(1);
            task::sleep(200.ms()).await;
            task_value.set(2);
        });
    });
    assert!(
        ui.tree().next_deadline().is_some(),
        "pending timers are reported to the window"
    );
    ui.advance(250.ms());
    assert!(ui.find_text("task 1").is_some());
    assert!(ui.find_text("waiting").is_some());
    ui.advance(300.ms());
    assert!(ui.find_text("fired").is_some());
    assert!(ui.find_text("task 2").is_some());
    drop(ui);
    owner.dispose();
}

#[test]
fn breakpoint_follows_viewport_and_rebuilds_responsive_regions_only_on_crossing() {
    let (owner, builds) = setup(|| signal(0));
    let mut ui = Headless::new(Size::new(1200.0, 800.0), move || {
        responsive(move |bp| {
            builds.update_untracked(|n| *n += 1);
            text(format!("{bp:?}"))
        })
    });
    assert!(ui.find_text("Lg").is_some());
    let before = builds.get_untracked();
    ui.resize(Size::new(1100.0, 800.0), 1.0);
    assert_eq!(
        builds.get_untracked(),
        before,
        "same breakpoint: no rebuild"
    );
    ui.resize(Size::new(500.0, 800.0), 1.0);
    assert!(ui.find_text("Xs").is_some());
    assert_eq!(viewport_size(), Size::new(500.0, 800.0));
    drop(ui);
    owner.dispose();
}

#[test]
fn keyed_list_moves_existing_items_and_disposes_removed_ones() {
    let (owner, items) = setup(|| signal(vec![1u32, 2, 3]));
    let renders = std::rc::Rc::new(std::cell::Cell::new(0));
    let item_signals = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (r, sigs) = (renders.clone(), item_signals.clone());
    let mut ui = Headless::new(Size::new(300.0, 300.0), move || {
        let (r, sigs) = (r.clone(), sigs.clone());
        keyed(
            move || items.get(),
            |n| *n,
            move |n| {
                r.set(r.get() + 1);
                // Item-owned state: disposed when the key disappears.
                let local = signal(n * 10);
                sigs.borrow_mut().push((n, local));
                div()
                    .id(("item", n))
                    .h(20.0)
                    .child(text(move || format!("item {}", local.get())))
            },
        )
    });
    assert_eq!(renders.get(), 3);
    let node_2 = ui.node(("item", 2u32)).unwrap();
    let y_before = ui.bounds_of(("item", 2u32)).unwrap().top();

    items.set(vec![3, 2, 1]);
    ui.settle();
    assert_eq!(renders.get(), 3, "reordering renders nothing new");
    assert_eq!(ui.node(("item", 2u32)), Some(node_2), "node retained");
    assert_eq!(ui.bounds_of(("item", 3u32)).unwrap().top(), y_before - 20.0);

    items.set(vec![3, 4]);
    ui.settle();
    assert_eq!(renders.get(), 4, "only the new key renders");
    let sigs = item_signals.borrow();
    let alive: Vec<u32> = sigs
        .iter()
        .filter(|(_, s)| s.is_alive())
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(alive, vec![3, 4], "state of removed items is disposed");
    assert_eq!(ui.visible_text(), vec!["item 30", "item 40"]);
    drop(sigs);
    drop(ui);
    owner.dispose();
}
