//! Behavior of the overlay widgets, driven through the headless harness.

use crate::headless::Headless;
use crate::icons;
use crate::prelude::*;
use kova_native_core::{DurationExt, Owner, Point, Size, signal};

fn run<T: Copy + 'static, E: IntoElement>(
    state: impl FnOnce() -> T,
    build: impl Fn(T) -> E + 'static,
    test: impl FnOnce(&mut Headless, T),
) {
    let owner = Owner::new_root();
    let state = owner.with(state);
    let mut ui = Headless::new(Size::new(900.0, 640.0), move || build(state));
    test(&mut ui, state);
    drop(ui);
    owner.dispose();
}

#[test]
fn dialog_opens_traps_focus_and_closes_with_escape_or_backdrop() {
    run(
        || signal(false),
        |open| {
            column()
                .padding(40.0)
                .child(
                    button("Open dialog")
                        .id("open")
                        .on_click(move |_| open.set(true)),
                )
                .child(
                    dialog(open)
                        .title("Rename project")
                        .description("Visible to the workspace.")
                        .content(|| text("Body text"))
                        .footer(move || {
                            button("Done").id("done").on_click(move |_| open.set(false))
                        }),
                )
        },
        |ui, open| {
            ui.click_id("open");
            assert!(open.get());
            assert!(ui.find_text("Rename project").is_some());
            assert!(ui.find_text("Body text").is_some());
            // The panel holds focus (Escape works without clicking first).
            ui.press("escape");
            assert!(!open.get());
            assert_eq!(
                ui.focused_id(),
                Some("open".into()),
                "focus returns to the opener"
            );

            ui.click_id("open");
            ui.press("tab");
            ui.press("tab");
            ui.press("tab");
            let focused = ui.focused_id();
            assert!(
                focused == Some("dialog-close".into()) || focused == Some("done".into()),
                "Tab stays inside the dialog: {focused:?}"
            );
            // A click on the backdrop (outside the panel) closes it.
            ui.click_at(Point::new(10.0, 10.0));
            assert!(!open.get());
            assert!(ui.find_text("Rename project").is_none());
        },
    );
}

#[test]
fn confirm_dialog_focuses_cancel_and_runs_the_action() {
    run(
        || (signal(true), signal(0)),
        |(open, deleted)| {
            confirm_dialog(
                open,
                "Delete key?",
                "This cannot be undone.",
                "Delete",
                move || deleted.update(|n| *n += 1),
            )
        },
        |ui, (open, deleted)| {
            assert_eq!(ui.focused_id(), Some("confirm-cancel".into()));
            ui.press("enter");
            assert!(!open.get(), "Enter on the focused Cancel closes");
            assert_eq!(deleted.get(), 0);
            open.set(true);
            ui.settle();
            ui.click_id("confirm-accept");
            assert!(!open.get());
            assert_eq!(deleted.get(), 1);
        },
    );
}

#[test]
fn dropdown_menu_supports_mouse_and_keyboard() {
    run(
        || signal(Vec::<&'static str>::new()),
        |log| {
            column().padding(40.0).child(dropdown_menu(
                button("Actions").id("actions"),
                move || {
                    vec![
                        menu_heading("Project"),
                        menu_item("Rename", move || log.update(|l| l.push("rename")))
                            .icon(icons::EDIT)
                            .shortcut("F2"),
                        menu_item("Archived", || {}).disabled(true),
                        menu_separator(),
                        menu_item("Delete", move || log.update(|l| l.push("delete")))
                            .icon(icons::TRASH)
                            .danger(),
                    ]
                },
            ))
        },
        |ui, log| {
            ui.click_id("actions");
            assert!(ui.find_text("Rename").is_some());
            ui.click_text("Delete");
            assert_eq!(log.get(), ["delete"]);
            assert!(
                ui.find_text("Rename").is_none(),
                "menu closes after a choice"
            );
            assert_eq!(
                ui.focused_id(),
                Some("actions".into()),
                "focus back on the trigger"
            );

            // Keyboard: ↓ opens (first item highlighted), disabled items are skipped.
            ui.press("down");
            assert!(ui.find_text("Rename").is_some());
            ui.press("down");
            ui.press("enter");
            assert_eq!(log.get(), ["delete", "delete"], "↓ skips the disabled item");
            ui.press("down");
            ui.press("escape");
            assert!(ui.find_text("Rename").is_none());
            // Clicking outside closes without running anything.
            ui.click_id("actions");
            ui.click_at(Point::new(800.0, 600.0));
            assert!(ui.find_text("Rename").is_none());
            assert_eq!(log.get().len(), 2);
        },
    );
}

#[test]
fn select_changes_value_and_matches_trigger_width() {
    run(
        || signal(1usize),
        |selected| {
            column().padding(40.0).child(
                select(["Development", "Staging", "Production"], selected)
                    .id("env")
                    .width(240.0),
            )
        },
        |ui, selected| {
            assert!(ui.find_text("Staging").is_some());
            ui.click_id("env");
            ui.click_text("Production");
            assert_eq!(selected.get(), 2);
            assert_eq!(
                ui.visible_text()
                    .iter()
                    .filter(|t| *t == "Production")
                    .count(),
                1
            );

            ui.click_id("env");
            let trigger = ui.bounds_of("env").unwrap();
            let item = ui.find_text("Development").unwrap();
            let list = ui.tree().visual_bounds(item).unwrap();
            assert!(list.top() > trigger.bottom(), "list below the trigger");
            // The highlighted entry starts at the current value; ↑ ↑ Enter picks the first.
            ui.press("up");
            ui.press("up");
            ui.press("enter");
            assert_eq!(selected.get(), 0);
        },
    );
}

#[test]
fn context_menu_opens_at_the_pointer() {
    run(
        || signal(0),
        |copied| {
            column().padding(40.0).child(context_menu(
                div().id("area").w(300.0).h(200.0).bg(0x202020),
                move || vec![menu_item("Copy", move || copied.update(|n| *n += 1))],
            ))
        },
        |ui, copied| {
            let area = ui.bounds_of("area").unwrap();
            let at = Point::new(area.left() + 50.0, area.top() + 40.0);
            ui.right_click_at(at);
            let item = ui.find_text("Copy").expect("menu shown");
            let b = ui.tree().visual_bounds(item).unwrap();
            assert!(b.left() > at.x && b.top() > at.y && b.left() < at.x + 30.0);
            ui.click_text("Copy");
            assert_eq!(copied.get(), 1);
            ui.right_click_at(at);
            ui.click_at(Point::new(800.0, 600.0));
            assert!(ui.find_text("Copy").is_none());
        },
    );
}

#[test]
fn tooltip_appears_after_a_delay_and_hides_on_leave() {
    run(
        || (),
        |()| {
            column()
                .padding(80.0)
                .child(tooltip(icon_button(icons::SETTINGS).id("gear"), "Settings"))
        },
        |ui, ()| {
            ui.hover_id("gear");
            assert!(ui.find_text("Settings").is_none());
            ui.advance(TOOLTIP_DELAY + 50.ms());
            let tip = ui.find_text("Settings").expect("tooltip after delay");
            let gear = ui.bounds_of("gear").unwrap();
            assert!(ui.tree().visual_bounds(tip).unwrap().bottom() <= gear.top());
            ui.mouse_move(Point::new(600.0, 500.0));
            assert!(ui.find_text("Settings").is_none());
        },
    );
}

#[test]
fn toasts_stack_auto_dismiss_and_cap() {
    run(
        || (),
        |()| column().child(text("App")).child(toaster()),
        |ui, ()| {
            toast("Saved")
                .success()
                .description("All changes stored")
                .show();
            let sticky = toast("Sticky").persistent().show();
            ui.settle();
            assert!(ui.find_text("Saved").is_some());
            assert!(ui.find_text("All changes stored").is_some());
            ui.advance(4100.ms());
            assert!(ui.find_text("Saved").is_none(), "auto-dismissed");
            assert!(ui.find_text("Sticky").is_some());
            for i in 0..6 {
                toast(format!("n{i}")).show();
            }
            ui.settle();
            assert_eq!(toast_count(), MAX_TOASTS);
            assert!(ui.find_text("n5").is_some());
            dismiss_toast(sticky);
            ui.advance(5.secs());
            assert_eq!(toast_count(), 0);
        },
    );
}

#[test]
fn command_palette_filters_navigates_and_runs() {
    run(
        || (signal(true), signal("")),
        |(open, last)| {
            column().child(command_palette(open, move || {
                vec![
                    command("Open settings", move || last.set("settings")).group("General"),
                    command("Toggle theme", move || last.set("theme")).shortcut("Ctrl+T"),
                    command("New project", move || last.set("project")).icon(icons::PLUS),
                    command("Sign out", move || last.set("out")).keywords("logout exit"),
                ]
            }))
        },
        |ui, (open, last)| {
            assert_eq!(ui.focused_id(), Some("palette-query".into()));
            ui.type_text("tt");
            assert!(ui.find_text("Toggle theme").is_some());
            assert!(ui.find_text("New project").is_none());
            ui.press("enter");
            assert_eq!(last.get(), "theme");
            assert!(!open.get());

            open.set(true);
            ui.settle();
            ui.type_text("logout");
            assert!(ui.find_text("Sign out").is_some(), "keywords match");
            ui.press("escape");
            assert!(!open.get());

            open.set(true);
            ui.settle();
            ui.press("down");
            ui.press("down");
            ui.press("enter");
            assert_eq!(last.get(), "project", "arrow keys move through results");
        },
    );
}

#[derive(Clone)]
struct Row {
    id: u32,
    name: &'static str,
    seats: u32,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            id: 1,
            name: "Bravo",
            seats: 3,
        },
        Row {
            id: 2,
            name: "Alpha",
            seats: 12,
        },
        Row {
            id: 3,
            name: "Charlie",
            seats: 7,
        },
    ]
}

#[test]
fn data_table_sorts_by_header_and_reports_row_clicks() {
    run(
        || (signal(rows()), signal(None::<u32>)),
        |(data, picked)| {
            column().padding(20.0).child(
                data_table(
                    move || data.get(),
                    |r| r.id,
                    vec![
                        table_column("Name", |r: &Row| text(r.name)).sortable_by(|r| r.name),
                        table_column("Seats", |r: &Row| text(r.seats.to_string()))
                            .width(80.0)
                            .align_end()
                            .sortable_by(|r| r.seats),
                    ],
                )
                .on_row_click(move |r| picked.set(Some(r.id)))
                .row_active(move |r| picked.get() == Some(r.id))
                .empty_state(|| text("Nothing here")),
            )
        },
        |ui, (data, picked)| {
            let order = |ui: &Headless| -> Vec<String> {
                ui.visible_text()
                    .into_iter()
                    .filter(|t| ["Alpha", "Bravo", "Charlie"].contains(&t.as_str()))
                    .collect()
            };
            assert_eq!(order(ui), ["Bravo", "Alpha", "Charlie"]);
            let alpha = ui.find_text("Alpha").unwrap();
            ui.click_id(("table-sort", 0usize));
            assert_eq!(order(ui), ["Alpha", "Bravo", "Charlie"]);
            assert_eq!(
                ui.find_text("Alpha"),
                Some(alpha),
                "sorting moves retained rows"
            );
            ui.click_id(("table-sort", 0usize));
            assert_eq!(order(ui), ["Charlie", "Bravo", "Alpha"]);
            ui.click_id(("table-sort", 1usize));
            assert_eq!(
                order(ui),
                ["Bravo", "Charlie", "Alpha"],
                "by seats ascending"
            );

            ui.click_text("Charlie");
            assert_eq!(picked.get(), Some(3));
            data.set(Vec::new());
            ui.settle();
            assert!(ui.find_text("Nothing here").is_some());
        },
    );
}

#[test]
fn pagination_moves_between_pages_and_disables_edges() {
    run(
        || signal(0usize),
        |page| column().padding(20.0).child(pagination(page, || 12)),
        |ui, page| {
            assert!(ui.find_text("12").is_some());
            ui.click_text("2");
            assert_eq!(page.get(), 1);
            ui.click_text("12");
            assert_eq!(page.get(), 11);
            assert!(
                ui.find_text("10").is_some(),
                "window follows the current page"
            );
            assert!(ui.find_text("4").is_none());
        },
    );
}

#[test]
fn breadcrumbs_report_all_but_the_current_crumb() {
    run(
        || signal(None::<usize>),
        |picked| {
            column()
                .padding(20.0)
                .child(breadcrumbs(["Workspace", "Projects", "kova"], move |i| {
                    picked.set(Some(i))
                }))
        },
        |ui, picked| {
            ui.click_text("Projects");
            assert_eq!(picked.get(), Some(1));
            picked.set(None);
            ui.click_text("kova");
            assert_eq!(picked.get(), None, "current crumb is not a link");
        },
    );
}

#[test]
fn code_block_copies_its_source_and_search_clears() {
    run(
        || signal(String::from("query")),
        |value| {
            column()
                .padding(20.0)
                .gap(10.0)
                .child(code_block("cargo run -p overlays\n").language("bash"))
                .child(search_input(value).input(|i| i.id("search")))
        },
        |ui, value| {
            assert!(ui.find_text("cargo run -p overlays").is_some());
            assert!(ui.find_text("bash").is_some());
            let copy = ui
                .tree()
                .painted_nodes()
                .into_iter()
                .find(|n| {
                    ui.tree().element_name(*n) == Some("div") && {
                        let b = ui.tree().visual_bounds(*n).unwrap();
                        b.width() == 28.0 && b.height() == 28.0
                    }
                })
                .expect("copy button");
            let at = ui.tree().visual_bounds(copy).unwrap().center();
            ui.click_at(at);
            assert_eq!(ui.clipboard(), Some("cargo run -p overlays"));

            let search = ui.bounds_of("search").unwrap();
            ui.click_at(kova_native_core::Point::new(
                search.right() - 16.0,
                search.center().y,
            ));
            assert_eq!(value.get(), "", "the clear button empties the field");
        },
    );
}
