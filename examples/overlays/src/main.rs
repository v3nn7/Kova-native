//! The overlay layer: dialogs, drawers, menus, selects, popovers, tooltips,
//! toasts and a command palette (Ctrl+K).
//!
//! ```text
//! cargo run -p overlays
//! cargo run -p overlays --release -- --record docs/media/overlays.gif
//! ```

use kova_native::prelude::*;
use kova_native::record::Recorder;

actions!(OpenPalette);

#[derive(Clone, Copy)]
struct State {
    dialog: Signal<bool>,
    confirm: Signal<bool>,
    drawer: Signal<bool>,
    palette: Signal<bool>,
    popover: Signal<bool>,
    project_name: Signal<String>,
    environment: Signal<usize>,
    region: Signal<usize>,
    notify_email: Signal<bool>,
    notify_slack: Signal<bool>,
    log: Signal<Vec<String>>,
}

impl State {
    fn new() -> Self {
        Self {
            dialog: signal(false),
            confirm: signal(false),
            drawer: signal(false),
            palette: signal(false),
            popover: signal(false),
            project_name: signal("kova-demo".into()),
            environment: signal(0),
            region: signal(1),
            notify_email: signal(true),
            notify_slack: signal(false),
            log: signal(vec!["Ready.".into()]),
        }
    }

    fn record(self, entry: impl Into<String>) {
        self.log.update(|log| {
            log.push(entry.into());
            let excess = log.len().saturating_sub(5);
            log.drain(..excess);
        });
    }
}

const ENVIRONMENTS: [&str; 3] = ["Development", "Staging", "Production"];
const REGIONS: [&str; 4] = ["eu-central-1", "eu-west-2", "us-east-1", "ap-southeast-1"];

fn section(title: &'static str, description: &'static str) -> Div {
    let t = theme();
    column()
        .gap(12.0)
        .padding(16.0)
        .bg(t.surface)
        .border(1.0)
        .border_color(t.border)
        .rounded(t.radius)
        .child(
            column()
                .gap(3.0)
                .child(text(title).size(13.5).semibold())
                .child(text(description).size(12.5).color(t.text_muted)),
        )
}

fn top_bar(state: State) -> Div {
    let t = theme();
    row()
        .gap(10.0)
        .px(20.0)
        .py(10.0)
        .border_b(1.0)
        .border_color(t.border)
        .bg(t.surface)
        .child(
            div()
                .size(26.0)
                .center()
                .rounded(7.0)
                .bg(t.accent)
                .child(icon(icons::GRID).size(15.0).color(Color::WHITE)),
        )
        .child(breadcrumbs(["Kova", "Examples", "Overlays"], move |i| {
            state.record(format!("Breadcrumb {i}"))
        }))
        .child(spacer())
        .child(
            row()
                .id("palette-hint")
                .gap(8.0)
                .px(10.0)
                .py(5.0)
                .w(230.0)
                .rounded(t.radius_small)
                .bg(t.surface_sunken)
                .border(1.0)
                .border_color(t.border)
                .text_color(t.text_subtle)
                .text_size(12.5)
                .cursor_pointer()
                .hover({
                    let c = t.border_strong;
                    move |s| s.border_color(c)
                })
                .on_click(move |_| state.palette.set(true))
                .child(icon(icons::SEARCH).size(14.0))
                .child(text("Search commands…").flex_1())
                .child(kbd("Ctrl K")),
        )
        .child(tooltip(
            icon_button(icons::BELL).id("bell").on_click(move |_| {
                toast("No new notifications").show();
            }),
            "Notifications",
        ))
        .child(dropdown_menu(
            row()
                .id("account")
                .gap(8.0)
                .cursor_pointer()
                .child(avatar("VN").size(28.0))
                .child(icon(icons::CHEVRON_DOWN).size(14.0).color(t.text_muted)),
            move || {
                vec![
                    menu_heading("v3nn7"),
                    menu_item("Profile", move || state.record("Opened profile")).icon(icons::USER),
                    menu_item("Settings", move || state.drawer.set(true))
                        .icon(icons::SETTINGS)
                        .shortcut("Ctrl ,"),
                    menu_item("Switch theme", toggle_theme).icon(icons::MOON),
                    menu_separator(),
                    menu_item("Sign out", move || state.record("Signed out"))
                        .icon(icons::LOG_OUT)
                        .danger(),
                ]
            },
        ))
}

fn toggle_theme() {
    let accent = theme().accent;
    let next = if theme().dark {
        Theme::light()
    } else {
        Theme::dark()
    };
    set_theme(next.with_accent(accent));
}

fn dialogs(state: State) -> Div {
    section(
        "Dialogs",
        "Modal layers trap focus and return it when they close.",
    )
    .child(
        row()
            .gap(8.0)
            .flex_wrap()
            .child(
                button("Edit project")
                    .id("open-dialog")
                    .icon(icon(icons::EDIT))
                    .on_click(move |_| state.dialog.set(true)),
            )
            .child(
                button("Open drawer")
                    .secondary()
                    .id("open-drawer")
                    .on_click(move |_| state.drawer.set(true)),
            )
            .child(
                button("Delete…")
                    .outline()
                    .id("open-confirm")
                    .on_click(move |_| state.confirm.set(true)),
            ),
    )
    .child(
        row()
            .gap(6.0)
            .text_size(12.5)
            .text_color(theme().text_muted)
            .child(text("Project:"))
            .child(text(move || state.project_name.get()).color(theme().text))
            .child(text("·"))
            .child(text(move || {
                ENVIRONMENTS[state.environment.get()].to_string()
            })),
    )
}

fn menus(state: State) -> Div {
    let t = theme();
    section(
        "Menus",
        "Keyboard navigable, flip near edges, never clipped.",
    )
    .child(
        row().gap(8.0).child(dropdown_menu(
            button("Actions")
                .secondary()
                .id("actions")
                .icon(icon(icons::MORE_HORIZONTAL)),
            move || {
                vec![
                    menu_item("Duplicate", move || state.record("Duplicated"))
                        .icon(icons::COPY)
                        .shortcut("Ctrl D"),
                    menu_item("Export", move || {
                        toast("Export started")
                            .description("You will be notified when it finishes.")
                            .show();
                    })
                    .icon(icons::DOWNLOAD),
                    menu_item("Archive", || {})
                        .icon(icons::INBOX)
                        .disabled(true),
                    menu_separator(),
                    menu_item("Delete project", move || state.confirm.set(true))
                        .icon(icons::TRASH)
                        .danger(),
                ]
            },
        )),
    )
    .child(
        context_menu(
            column()
                .id("context-area")
                .h(92.0)
                .w_full()
                .center()
                .gap(4.0)
                .rounded(t.radius_small)
                .border(1.0)
                .border_color(t.border_strong)
                .bg(t.surface_sunken)
                .text_color(t.text_subtle)
                .text_size(12.5)
                .child(icon(icons::FILE).size(18.0))
                .child(text("Right-click for a context menu")),
            move || {
                vec![
                    menu_item("Open", move || state.record("Context: open"))
                        .icon(icons::EXTERNAL_LINK),
                    menu_item("Copy path", move || state.record("Context: copy path"))
                        .icon(icons::LINK),
                    menu_separator(),
                    menu_item("Rename", move || state.dialog.set(true)).icon(icons::EDIT),
                ]
            },
        )
        .w_full(),
    )
}

fn inputs(state: State) -> Div {
    section(
        "Selects & popovers",
        "Anchored to their trigger, sized to match it.",
    )
    .child(
        column()
            .gap(6.0)
            .child(label("Environment").text_size(12.0))
            .child(select(ENVIRONMENTS, state.environment).id("environment")),
    )
    .child(
        column()
            .gap(6.0)
            .child(label("Region").text_size(12.0))
            .child(select(REGIONS, state.region).id("region")),
    )
    .child(
        popover(
            state.popover,
            button("Notification settings")
                .ghost()
                .small()
                .id("popover-trigger")
                .icon(icon(icons::SLIDERS)),
            move || {
                column()
                    .gap(10.0)
                    .w(240.0)
                    .child(text("Deliver alerts to").size(12.5).semibold())
                    .child(
                        row()
                            .justify_between()
                            .child(text("Email").size(13.0))
                            .child(switch(state.notify_email)),
                    )
                    .child(
                        row()
                            .justify_between()
                            .child(text("Slack").size(13.0))
                            .child(switch(state.notify_slack)),
                    )
            },
        )
        .placement(Placement::TOP_START),
    )
}

fn notifications(state: State) -> Div {
    section("Toasts", "Queued, timed and dismissible; actions run once.")
        .child(
            row()
                .gap(8.0)
                .flex_wrap()
                .child(
                    button("Success")
                        .secondary()
                        .small()
                        .id("toast-success")
                        .on_click(|_| {
                            toast("Deployment finished")
                                .success()
                                .description("kova-demo is live in eu-central-1.")
                                .show();
                        }),
                )
                .child(button("Warning").secondary().small().on_click(|_| {
                    toast("Usage at 85%")
                        .warning()
                        .description("Consider raising the limit.")
                        .show();
                }))
                .child(
                    button("With undo")
                        .secondary()
                        .small()
                        .id("toast-undo")
                        .on_click(move |_| {
                            toast("Key revoked")
                                .danger()
                                .action("Undo", move || state.record("Revocation undone"))
                                .show();
                        }),
                ),
        )
        .child(
            column()
                .gap(4.0)
                .text_size(12.0)
                .font_family(kova_native::text::MONOSPACE)
                .text_color(theme().text_muted)
                .child(keyed(
                    move || state.log.get().into_iter().enumerate().collect::<Vec<_>>(),
                    |(i, line)| (*i, line.clone()),
                    |(_, line)| text(format!("› {line}")),
                )),
        )
}

fn project_dialog(state: State) -> Dialog {
    let draft = signal(String::new());
    dialog(state.dialog)
        .title("Edit project")
        .description("Changes apply to every environment.")
        .content(move || {
            draft.set(state.project_name.get_untracked());
            column()
                .gap(12.0)
                .child(
                    column()
                        .gap(6.0)
                        .child(label("Name").text_size(12.0))
                        .child(
                            text_input(draft)
                                .id("project-name")
                                .autofocus()
                                .max_chars(40),
                        ),
                )
                .child(
                    column()
                        .gap(6.0)
                        .child(label("Default environment").text_size(12.0))
                        .child(select(ENVIRONMENTS, state.environment)),
                )
        })
        .footer(move || {
            row()
                .gap(8.0)
                .child(
                    button("Cancel")
                        .secondary()
                        .on_click(move |_| state.dialog.set(false)),
                )
                .child(
                    button("Save changes")
                        .id("save-project")
                        .on_click(move |_| {
                            let name = draft.get_untracked();
                            if name.trim().is_empty() {
                                toast("Name required").warning().show();
                                return;
                            }
                            state.project_name.set(name.clone());
                            state.dialog.set(false);
                            state.record(format!("Renamed to {name}"));
                            toast("Project saved").success().description(name).show();
                        }),
                )
        })
}

fn settings_drawer(state: State) -> Region {
    drawer(state.drawer, Side::Right, 360.0, move || {
        let t = theme();
        column()
            .size_full()
            .child(
                row()
                    .justify_between()
                    .px(18.0)
                    .py(14.0)
                    .border_b(1.0)
                    .border_color(t.border)
                    .child(text("Settings").size(15.0).semibold())
                    .child(icon_button(icons::CLOSE).on_click(move |_| state.drawer.set(false))),
            )
            .child(
                column()
                    .gap(14.0)
                    .padding(18.0)
                    .child(
                        row()
                            .justify_between()
                            .child(text("Email alerts").size(13.0))
                            .child(switch(state.notify_email)),
                    )
                    .child(
                        row()
                            .justify_between()
                            .child(text("Slack alerts").size(13.0))
                            .child(switch(state.notify_slack)),
                    )
                    .child(divider())
                    .child(
                        button("Switch theme")
                            .secondary()
                            .icon(icon(icons::SUN))
                            .on_click(|_| toggle_theme()),
                    ),
            )
    })
}

fn palette(state: State) -> Region {
    command_palette(state.palette, move || {
        let mut commands = vec![
            command("Edit project", move || state.dialog.set(true))
                .group("Project")
                .icon(icons::EDIT),
            command("Delete project", move || state.confirm.set(true))
                .group("Project")
                .icon(icons::TRASH),
            command("Open settings", move || state.drawer.set(true))
                .group("General")
                .icon(icons::SETTINGS)
                .shortcut("Ctrl ,"),
            command("Toggle theme", toggle_theme)
                .group("Appearance")
                .icon(icons::MOON)
                .keywords("dark light"),
            command("Show a toast", || {
                toast("Hello from the palette").show();
            })
            .group("Demo")
            .icon(icons::BELL),
        ];
        for (i, env) in ENVIRONMENTS.iter().enumerate() {
            commands.push(
                command(format!("Switch to {env}"), move || {
                    state.environment.set(i);
                    toast(format!("Environment: {env}")).show();
                })
                .group("Environment")
                .icon(icons::SERVER),
            );
        }
        commands
    })
}

fn app(state: State) -> Div {
    let t = theme();
    column()
        .size_full()
        .bg(t.background)
        .on_action(move |_: &OpenPalette, _| state.palette.set(true))
        .child(top_bar(state))
        .child(
            div()
                .flex_1()
                .min_h(0.0)
                .overflow_y_scroll()
                .padding(20.0)
                .child(
                    div()
                        .w_full()
                        .grid_cols(2)
                        .gap(14.0)
                        .bind(|s| {
                            if breakpoint() < Breakpoint::Md {
                                s.grid_cols(1)
                            } else {
                                s
                            }
                        })
                        .child(dialogs(state))
                        .child(menus(state))
                        .child(inputs(state))
                        .child(notifications(state)),
                ),
        )
        .child(project_dialog(state))
        .child(confirm_dialog(
            state.confirm,
            "Delete kova-demo?",
            "The project, its environments and all keys are removed permanently.",
            "Delete project",
            move || {
                state.record("Project deleted");
                toast("Project deleted").danger().show();
            },
        ))
        .child(settings_drawer(state))
        .child(palette(state))
        .child(toaster())
}

fn record(state: State, path: &str) -> KovaResult<()> {
    let ui = Headless::new(Size::new(980.0, 620.0), move || app(state));
    let mut rec = Recorder::new(ui, 1.0)?;
    rec.hold(500.ms())?;
    // Dialog with a form and a select inside it.
    rec.click_id("open-dialog")?;
    rec.hold(300.ms())?;
    rec.act(|ui| ui.press("ctrl-a"))?;
    rec.type_text("kova-native")?;
    rec.hold(250.ms())?;
    rec.click_id("save-project")?;
    rec.hold(900.ms())?;
    // Dropdown menu.
    rec.click_id("actions")?;
    rec.hold(350.ms())?;
    rec.press("down")?;
    rec.hold(200.ms())?;
    rec.click_text("Export")?;
    rec.hold(700.ms())?;
    // Select.
    rec.click_id("region")?;
    rec.hold(300.ms())?;
    rec.click_text("us-east-1")?;
    rec.hold(400.ms())?;
    // Context menu.
    let area = rec.ui().bounds_of("context-area").expect("context area");
    rec.move_to(area.center(), 450.ms())?;
    rec.act(|ui| ui.right_click_at(area.center()))?;
    rec.hold(700.ms())?;
    rec.press("escape")?;
    // Tooltip.
    rec.hover_id("bell")?;
    rec.hold(800.ms())?;
    // Command palette.
    rec.click_id("palette-hint")?;
    rec.hold(250.ms())?;
    rec.type_text("theme")?;
    rec.hold(300.ms())?;
    rec.press("enter")?;
    rec.hold(600.ms())?;
    rec.act(|ui| ui.press("ctrl-k"))?;
    rec.type_text("prod")?;
    rec.hold(250.ms())?;
    rec.press("enter")?;
    rec.hold(900.ms())?;
    // Confirmation.
    rec.click_id("open-confirm")?;
    rec.hold(900.ms())?;
    rec.click_id("confirm-cancel")?;
    rec.hold(600.ms())?;
    rec.save_gif(path)?;
    println!(
        "Recorded {} frames ({:.1}s) to {path}",
        rec.frame_count(),
        rec.duration().as_secs_f32()
    );
    Ok(())
}

fn main() -> KovaResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let owner = Owner::new_root();
    let state = owner.with(State::new);
    let result = if let Some(i) = args.iter().position(|a| a == "--record") {
        let path = args
            .get(i + 1)
            .ok_or_else(|| KovaError::Other("usage: overlays --record <out.gif>".into()))?;
        record(state, path)
    } else {
        let app_window = Application::new()
            .title("Kova Native / Overlays")
            .size(1040.0, 680.0)
            .key_bindings([KeyBinding::new("ctrl-k", OpenPalette)]);
        let app_window = if args.iter().any(|a| a == "--smoke") {
            app_window.run_for(1.5.secs())
        } else {
            app_window
        };
        app_window.run(move || app(state)).map(|report| {
            println!("Native run: {report:?}");
        })
    };
    owner.dispose();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness() -> (Owner, State, Headless) {
        let owner = Owner::new_root();
        let state = owner.with(State::new);
        let mut ui = Headless::new(Size::new(980.0, 620.0), move || app(state));
        ui.key_bindings([KeyBinding::new("ctrl-k", OpenPalette)]);
        (owner, state, ui)
    }

    #[test]
    fn edit_dialog_saves_the_name_and_shows_a_toast() {
        let (owner, state, mut ui) = harness();
        ui.click_id("open-dialog");
        assert_eq!(ui.focused_id(), Some("project-name".into()));
        ui.press("ctrl-a");
        ui.type_text("renamed");
        ui.click_id("save-project");
        assert_eq!(state.project_name.get(), "renamed");
        assert!(!state.dialog.get());
        assert!(ui.find_text("Project saved").is_some());
        drop(ui);
        owner.dispose();
    }

    #[test]
    fn shortcut_opens_the_palette_and_runs_a_command() {
        let (owner, state, mut ui) = harness();
        ui.press("ctrl-k");
        assert!(state.palette.get());
        ui.type_text("staging");
        ui.press("enter");
        assert_eq!(state.environment.get(), 1);
        drop(ui);
        owner.dispose();
    }

    #[test]
    fn confirm_and_menus_route_to_the_same_state() {
        let (owner, state, mut ui) = harness();
        ui.click_id("actions");
        ui.click_text("Delete project");
        assert!(state.confirm.get(), "menu item opened the confirmation");
        ui.click_id("confirm-accept");
        assert!(state.log.get().iter().any(|l| l == "Project deleted"));
        ui.click_id("region");
        ui.click_text("us-east-1");
        assert_eq!(state.region.get(), 2);
        drop(ui);
        owner.dispose();
    }

    #[test]
    fn compact_layout_below_the_md_breakpoint() {
        let (owner, _state, mut ui) = harness();
        let wide = ui.bounds_of("open-dialog").unwrap();
        ui.resize(Size::new(600.0, 700.0), 1.0);
        let narrow = ui.bounds_of("open-dialog").unwrap();
        assert!(narrow.width() > 0.0);
        let menus = ui.bounds_of("actions").unwrap();
        assert!(menus.top() > narrow.bottom(), "sections stack vertically");
        assert!(wide.top() <= ui.bounds_of("actions").unwrap().top());
        drop(ui);
        owner.dispose();
    }
}
