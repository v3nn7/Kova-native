use kova_native::prelude::*;

#[derive(Clone, Copy)]
struct Showcase {
    clicks: Signal<u32>,
    enabled: Signal<bool>,
    checked: Signal<bool>,
    amount: Signal<f32>,
    section: Signal<usize>,
    moved: Signal<bool>,
    accent: Signal<Option<u32>>,
    plan: Signal<usize>,
    tab: Signal<usize>,
    filters: [Signal<bool>; 4],
    seats: Signal<i32>,
    stars: Signal<u8>,
    faq: [Signal<bool>; 2],
}

impl Showcase {
    fn new() -> Self {
        Self {
            clicks: signal(0),
            enabled: signal(true),
            checked: signal(true),
            amount: signal(0.65),
            section: signal(0),
            moved: signal(false),
            accent: signal(None),
            plan: signal(1),
            tab: signal(0),
            filters: [signal(true), signal(false), signal(true), signal(false)],
            seats: signal(3),
            stars: signal(4),
            faq: [signal(true), signal(false)],
        }
    }
}

const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 4v16M19 4L8 12l11 8" stroke="#000" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" fill="none"/></svg>"##;
const ARROW: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 12h14m-6-6 6 6-6 6" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/></svg>"##;

fn eyebrow(label: &'static str) -> Text {
    text(label)
        .size(10.5)
        .semibold()
        .letter_spacing(1.5)
        .color(theme().text_subtle)
}

fn navigation(state: Showcase) -> Div {
    let t = theme();
    let (accent, selected, normal) = (t.accent_soft, t.text, t.text_muted);
    column()
        .w(208.0)
        .h_full()
        .flex_shrink_0()
        .padding(22.0)
        .gap(26.0)
        .bg(t.surface_sunken)
        .border_r(1.0)
        .border_color(t.border)
        .child(
            row()
                .gap(10.0)
                .child(
                    div()
                        .size(32.0)
                        .center()
                        .rounded(10.0)
                        .bg(t.accent)
                        .child(icon(LOGO).size(18.0).color(Color::WHITE)),
                )
                .child(
                    text("kova-native")
                        .size(19.0)
                        .semibold()
                        .letter_spacing(-0.7),
                ),
        )
        .child(
            column().gap(10.0).child(eyebrow("PLAYGROUND")).children(
                ["Overview", "Components", "Motion", "Typography"]
                    .into_iter()
                    .enumerate()
                    .map(move |(i, name)| {
                        row()
                            .id(("nav", i))
                            .w_full()
                            .px(12.0)
                            .py(10.0)
                            .gap(10.0)
                            .rounded(9.0)
                            .cursor_pointer()
                            .focusable()
                            .focus(move |s| s.border(1.0).border_color(selected))
                            .bind(move |s| {
                                s.bg(if state.section.get() == i {
                                    accent
                                } else {
                                    Color::TRANSPARENT
                                })
                                .text_color(
                                    if state.section.get() == i {
                                        selected
                                    } else {
                                        normal
                                    },
                                )
                            })
                            .transition(160.ms())
                            .on_click(move |_| state.section.set(i))
                            .child(text(format!("0{}", i + 1)).size(11.0).color(normal))
                            .child(text(name).size(13.0).medium())
                    }),
            ),
        )
        .child(spacer())
        .child(
            column().gap(10.0).child(eyebrow("APPEARANCE")).child(
                button(if t.dark {
                    "Switch to light"
                } else {
                    "Switch to dark"
                })
                .secondary()
                .small()
                .id("theme")
                .on_click(move |_| {
                    let base = if theme().dark {
                        Theme::light()
                    } else {
                        Theme::dark()
                    };
                    set_theme(match state.accent.get() {
                        Some(hex) => base.with_accent(rgb(hex)),
                        None => base,
                    })
                }),
            ),
        )
        .child(divider())
        .child(
            column()
                .gap(4.0)
                .child(text("Rust. Native. GPU.").size(12.0).medium())
                .child(label("Kova Native / 0.1.0").size(11.0)),
        )
}

fn hero() -> Div {
    let t = theme();
    stack()
        .h(188.0)
        .rounded(18.0)
        .overflow_hidden()
        .bg(linear_gradient(115.0, rgb(0x312353), rgb(0x151a33)))
        .child(
            div()
                .size(210.0)
                .absolute()
                .right(-30.0)
                .top(-70.0)
                .rounded_full()
                .bg(linear_gradient(160.0, rgb(0x9675ff), rgb(0x4431aa)))
                .rotate(-25.0),
        )
        .child(
            div()
                .size(118.0)
                .absolute()
                .right(120.0)
                .bottom(-55.0)
                .rounded(30.0)
                .bg(linear_gradient(30.0, rgb(0x2dd4bf), rgb(0x23566b)))
                .rotate(28.0),
        )
        .child(
            column()
                .absolute()
                .left(26.0)
                .top(25.0)
                .gap(9.0)
                .max_w(pct(70.0))
                .text_color(Color::WHITE)
                .child(
                    text("BUILT FOR THE DESKTOP")
                        .size(10.0)
                        .semibold()
                        .letter_spacing(2.0)
                        .color(rgb(0xc4b5fd)),
                )
                .child(
                    text("Small details.\nA different feeling.")
                        .size(30.0)
                        .semibold()
                        .line_height(1.12)
                        .letter_spacing(-0.8),
                )
                .child(
                    text("Retained elements, reactive state and pixels drawn on your GPU.")
                        .size(12.0)
                        .color(rgb(0xc3bdd9)),
                ),
        )
        .child(
            row()
                .absolute()
                .right(22.0)
                .bottom(20.0)
                .px(12.0)
                .py(8.0)
                .gap(8.0)
                .rounded(10.0)
                .bg(Color::WHITE.with_alpha(0.08))
                .border(1.0)
                .border_color(Color::WHITE.with_alpha(0.2))
                .backdrop_blur(14.0)
                .child(div().size(6.0).rounded_full().bg(t.success))
                .child(
                    text("Native GPU pipeline")
                        .size(11.0)
                        .medium()
                        .color(Color::WHITE),
                ),
        )
}

fn overview(state: Showcase) -> Div {
    let t = theme();
    column().gap(20.0).w_full()
        .child(hero())
        .child(row().justify_between().child(heading("Made to interact"))
            .child(badge("LIVE STATE")))
        .child(div().grid_cols(2).gap(16.0)
            .child(card().id("actions-card").gap(14.0)
                .child(eyebrow("01 / ACTIONS"))
                .child(text("One click, one update.").size(18.0).semibold())
                .child(label("The label below subscribes to its signal. The surrounding tree stays mounted.").size(12.0))
                .child(row().gap(9.0)
                    .child(button("Increment").id("increment").icon(icon(ARROW)).on_click(move |_| state.clicks.update(|v| *v += 1)))
                    .child(button("Reset").outline().id("reset").on_click(move |_| state.clicks.set(0))))
                .child(text(move || format!("{:02} interactions", state.clicks.get())).id("counter").size(27.0).medium().letter_spacing(-0.8))
                .child(row().gap(8.0)
                    .child(button("Secondary").secondary().small())
                    .child(button("Disabled").small().disabled(true).id("disabled"))))
            .child(card().gap(14.0)
                .child(eyebrow("02 / CONTROLS"))
                .child(text("State you can feel.").size(18.0).semibold())
                .child(row().justify_between().child(label("Enable interactions"))
                    .child(switch(state.enabled).id("enable")))
                .child(row().gap(10.0).child(checkbox(state.checked).id("check"))
                    .child(label("Keep the details")))
                .child(divider())
                .child(row().justify_between().child(label("Intensity"))
                    .child(text(move || format!("{:.0}%", state.amount.get() * 100.0)).medium()))
                .child(slider(state.amount).id("intensity").w_full())
                .child(progress_bar(move || state.amount.get()))))
        .child(row().justify_between().child(heading("Surface studies"))
            .child(label("Gradients / shadows / clipping").size(11.0)))
        .child(div().grid_cols(3).gap(14.0).children([
            ("Soft elevation", rgb(0x7864d9), rgb(0x5340ac)),
            ("Inset depth", rgb(0x268780), rgb(0x174e59)),
            ("Rounded geometry", rgb(0xc0874e), rgb(0x704662)),
        ].into_iter().enumerate().map(move |(i, (title, a, b))| {
            column().h(126.0).center().gap(9.0).rounded(14.0).bg(t.surface_raised)
                .child(div().w(82.0).h(48.0).rounded(if i == 2 { 24.0 } else { 12.0 }).bg(linear_gradient(125.0, a, b))
                    .shadow(if i == 1 {
                        BoxShadow::new(3.0, 10.0, Color::BLACK.with_alpha(0.55)).inset()
                    } else {
                        BoxShadow::new(7.0, 20.0, a.with_alpha(0.35))
                    }).hover(|s| s.scale(1.08).rotate(-4.0)).transition(Spring::snappy()))
                .child(label(title).size(11.0))
        })))
}

fn accent_picker(state: Showcase) -> Div {
    let t = theme();
    let ring = t.text;
    row().gap(10.0).children(
        Theme::ACCENTS
            .into_iter()
            .enumerate()
            .map(move |(i, (name, hex))| {
                let color = rgb(hex);
                tooltip(
                    div()
                        .id(("accent", i))
                        .size(28.0)
                        .rounded_full()
                        .bg(color)
                        .border(2.0)
                        .cursor_pointer()
                        .focusable()
                        .bind(move |s| {
                            if state.accent.get().unwrap_or(Theme::ACCENTS[0].1) == hex {
                                s.border_color(ring).scale(1.1)
                            } else {
                                s.border_color(Color::TRANSPARENT)
                            }
                        })
                        .hover(|s| s.scale(1.12))
                        .transition(Spring::snappy())
                        .on_click(move |_| {
                            state.accent.set(Some(hex));
                            let base = if theme().dark {
                                Theme::dark()
                            } else {
                                Theme::light()
                            };
                            set_theme(base.with_accent(color));
                        }),
                    name,
                )
            }),
    )
}

fn components(state: Showcase) -> Div {
    let t = theme();
    column()
        .gap(20.0)
        .w_full()
        .child(heading("Components, ready to use."))
        .child(label(
            "Every widget below follows the theme and the accent you pick.",
        ))
        .child(
            card().gap(14.0).child(eyebrow("ACCENT COLOR")).child(
                row().justify_between().child(accent_picker(state)).child(
                    row()
                        .gap(6.0)
                        .child(kbd("Tab"))
                        .child(label("then"))
                        .child(kbd("Enter")),
                ),
            ),
        )
        .child(
            div()
                .grid_cols(2)
                .gap(16.0)
                .child(
                    card()
                        .gap(14.0)
                        .child(eyebrow("01 / CHOICES"))
                        .child(text("Pick a plan").size(18.0).semibold())
                        .child(radio_group(&["Hobby", "Pro", "Team"], state.plan).id("plan"))
                        .child(divider())
                        .child(
                            row()
                                .justify_between()
                                .child(label("Seats"))
                                .child(stepper(state.seats, 1, 20).id("seats")),
                        )
                        .child(
                            row()
                                .justify_between()
                                .child(label("Your rating"))
                                .child(rating(state.stars, 5)),
                        ),
                )
                .child(
                    card()
                        .gap(14.0)
                        .child(eyebrow("02 / FILTERS"))
                        .child(tabs(&["All", "Design", "Engineering"], state.tab).id("tabs"))
                        .child(
                            row().gap(8.0).flex_wrap().children(
                                ["Rust", "GPU", "Layout", "Text"]
                                    .into_iter()
                                    .zip(state.filters)
                                    .map(|(name, sel)| chip(name, sel)),
                            ),
                        )
                        .child(dynamic(move || {
                            let names = [
                                "Every project",
                                "Mockups and motion studies",
                                "Renderer and layout work",
                            ];
                            label(names[state.tab.get().min(2)]).size(12.0)
                        }))
                        .child(
                            row()
                                .gap(10.0)
                                .items_center()
                                .child(
                                    row().children(
                                        ["Ada Lovelace", "Linus T", "Grace H"]
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, n)| {
                                                avatar(
                                                    &n.split_whitespace()
                                                        .filter_map(|w| w.chars().next())
                                                        .collect::<String>(),
                                                )
                                                .when(i > 0, |a| a.ml(-10.0))
                                            }),
                                    ),
                                )
                                .child(label("3 people are editing").size(12.0)),
                        ),
                ),
        )
        .child(
            column()
                .gap(10.0)
                .child(alert(
                    AlertKind::Info,
                    "Heads up",
                    "Kova Native widgets are drawn by your GPU, with no web view.",
                ))
                .child(alert(
                    AlertKind::Success,
                    "Saved",
                    "Your settings were stored.",
                ))
                .child(alert(
                    AlertKind::Warning,
                    "Almost full",
                    "The glyph atlas is at 90% of its budget.",
                ))
                .child(alert(
                    AlertKind::Danger,
                    "Build failed",
                    "Shader compilation reported one error.",
                )),
        )
        .child(
            div()
                .grid_cols(2)
                .gap(16.0)
                .child(
                    card()
                        .gap(10.0)
                        .child(eyebrow("03 / DETAILS"))
                        .child(accordion("What is Kova Native?", state.faq[0], || {
                            text("A native, GPU accelerated GUI framework for Rust.")
                        }))
                        .child(accordion("Does it need a browser?", state.faq[1], || {
                            text("No. There is no HTML, CSS runtime, JavaScript or WebView.")
                        })),
                )
                .child(
                    card()
                        .gap(12.0)
                        .child(eyebrow("04 / LOADING"))
                        .child(
                            row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .size(40.0)
                                        .rounded_full()
                                        .bg(t.surface_raised.mix(t.border, 0.6)),
                                )
                                .child(
                                    column()
                                        .flex_1()
                                        .gap(8.0)
                                        .child(skeleton().w(pct(60.0)))
                                        .child(skeleton()),
                                ),
                        )
                        .child(skeleton().h(56.0))
                        .child(
                            row()
                                .gap(10.0)
                                .child(spinner())
                                .child(label("Fetching data").size(12.0)),
                        ),
                ),
        )
}

fn motion(state: Showcase) -> Div {
    let t = theme();
    column()
        .w_full()
        .gap(20.0)
        .child(heading("Motion, with intention."))
        .child(label(
            "Retarget the spring mid-flight. Transform and color change on the retained element.",
        ))
        .child(
            card()
                .gap(24.0)
                .child(
                    row()
                        .gap(10.0)
                        .child(
                            button("Retarget spring")
                                .id("retarget")
                                .on_click(move |_| state.moved.toggle()),
                        )
                        .child(badge("SPRING / SNAPPY")),
                )
                .child(
                    div()
                        .h(180.0)
                        .w_full()
                        .relative()
                        .rounded(12.0)
                        .bg(t.surface_sunken)
                        .overflow_hidden()
                        .child(
                            div()
                                .id("spring-box")
                                .absolute()
                                .left(45.0)
                                .top(50.0)
                                .size(74.0)
                                .rounded(18.0)
                                .bg(t.accent)
                                .center()
                                .child(icon(LOGO).size(32.0).color(Color::WHITE))
                                .bind(move |s| {
                                    if state.moved.get() {
                                        s.translate_x(260.0).rotate(100.0).scale(1.15)
                                    } else {
                                        s
                                    }
                                })
                                .transition(Spring::snappy()),
                        ),
                ),
        )
        .child(
            card()
                .gap(12.0)
                .child(eyebrow("INPUT / TRANSFORMS"))
                .child(
                    row()
                        .h(88.0)
                        .px(24.0)
                        .gap(50.0)
                        .child(
                            slider(state.amount)
                                .id("tilted-slider")
                                .w(220.0)
                                .scale(1.1)
                                .rotate(-12.0),
                        )
                        .child(
                            text(move || format!("{:0.0}%", state.amount.get() * 100.0)).medium(),
                        ),
                )
                .child(label(
                    "Drag the tilted slider. Precision follows its scale and rotation.",
                )),
        )
        .child(
            card()
                .gap(16.0)
                .child(eyebrow("TIME / EASING"))
                .child(
                    row()
                        .gap(20.0)
                        .child(spinner())
                        .child(label("Three staggered, repeating animations")),
                )
                .child(
                    div()
                        .h(100.0)
                        .relative()
                        .overflow_hidden()
                        .rounded(12.0)
                        .bg(t.surface_sunken)
                        .child(
                            div()
                                .absolute()
                                .left(24.0)
                                .top(26.0)
                                .size(48.0)
                                .rounded(14.0)
                                .bg(linear_gradient(120.0, t.accent, t.success))
                                .animation(
                                    Animation::new(1.6.secs())
                                        .repeat()
                                        .alternate()
                                        .easing(Easing::EaseInOutCubic),
                                    |s, p| s.translate_x(320.0 * p).rotate(180.0 * p),
                                ),
                        ),
                ),
        )
}

fn typography() -> Div {
    column().w_full().gap(20.0)
        .child(heading("Words deserve good pixels."))
        .child(label("Advanced shaping, system fonts, wrapping and a shared glyph atlas."))
        .child(card().gap(16.0).child(eyebrow("UNICODE / FALLBACK"))
            .child(text("Zażółć gęślą jaźń.").size(30.0).semibold())
            .child(text("日本語 · Ελληνικά · العربية · ✓").size(25.0))
            .child(text("Ligatures: ffi · office · affinity").size(19.0))
            .child(divider())
            .child(label("This paragraph wraps when the window becomes smaller. Its retained TextLayout reuses shaping and width measurements; glyphs are rasterized at the current device scale.")))
        .child(card().gap(12.0).child(eyebrow("WEIGHT / DECORATION"))
            .child(text("Regular / 400").size(20.0))
            .child(text("Medium / 500").size(20.0).medium())
            .child(text("Semibold / 600").size(20.0).semibold())
            .child(text("Underline follows the shaped lines.").underline())
            .child(text("Strikethrough is painted by the existing Text element.").line_through()))
}

fn showcase(state: Showcase) -> Div {
    let t = theme();
    row()
        .size_full()
        .items_stretch()
        .bg(t.background)
        .text_color(t.text)
        .child(navigation(state))
        .child(
            column()
                .flex_1()
                .min_w(0.0)
                .h_full()
                .child(
                    row()
                        .h(66.0)
                        .flex_shrink_0()
                        .px(30.0)
                        .justify_between()
                        .border_b(1.0)
                        .border_color(t.border)
                        .child(text("Framework playground").size(13.0).medium())
                        .child(label("Tab to navigate · Enter to activate").size(11.0)),
                )
                .child(
                    column()
                        .id("content")
                        .flex_1()
                        .min_h(0.0)
                        .overflow_y_scroll()
                        .padding(28.0)
                        .gap(22.0)
                        .child(
                            dynamic(move || match state.section.get() {
                                1 => components(state),
                                2 => motion(state),
                                3 => typography(),
                                _ => overview(state),
                            })
                            .w_full(),
                        )
                        .child(
                            label("Rendered with wgpu. Built entirely with Kova Native elements.")
                                .size(11.0),
                        ),
                ),
        )
}

fn main() -> KovaResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let section = match args.iter().position(|a| a == "--page") {
        Some(i) => match args.get(i + 1).map(String::as_str) {
            Some("overview") => 0,
            Some("components") => 1,
            Some("motion") => 2,
            Some("typography") => 3,
            _ => {
                return Err(KovaError::Other(
                    "--page requires overview, components, motion or typography".into(),
                ));
            }
        },
        None => 0,
    };
    if args.iter().any(|a| a == "--light") {
        set_theme(Theme::light());
    }
    let owner = Owner::new_root();
    let state = owner.with(Showcase::new);
    state.section.set(section);
    let result = if args.first().is_some_and(|a| a == "--capture") {
        let path = args.get(1).ok_or_else(|| {
            KovaError::Other("usage: showcase --capture <output.png> [scale]".into())
        })?;
        let scale = args
            .get(2)
            .filter(|s| !s.starts_with("--"))
            .map(|s| s.parse::<f32>())
            .transpose()
            .map_err(|e| KovaError::Other(e.to_string()))?
            .unwrap_or(1.0);
        capture(state, path, scale)
    } else {
        let smoke = args.iter().any(|a| a == "--smoke");
        let mut options = WindowOptions::default();
        options.attributes.min_size = Some(Size::new(850.0, 600.0));
        let app = Application::new()
            .window(options)
            .title("Kova Native / Showcase")
            .size(1120.0, 820.0);
        let app = if smoke { app.run_for(2.0.secs()) } else { app };
        app.run(move || showcase(state)).and_then(|report| {
            if report.presented_frames == 0 {
                return Err(KovaError::Other("no native frames were presented".into()));
            }
            println!("Native run: {report:?}");
            Ok(())
        })
    };
    owner.dispose();
    result
}

fn capture(state: Showcase, path: &str, scale: f32) -> KovaResult<()> {
    if !scale.is_finite() || !(0.5..=3.0).contains(&scale) {
        return Err(KovaError::Other(
            "capture scale must be between 0.5 and 3".into(),
        ));
    }
    use kova_native::render::{GpuContext, Renderer, Scene};
    use kova_native::widgets::{ElementTree, FrameContext};
    let gpu = GpuContext::new_headless()?;
    let mut renderer = Renderer::new(&gpu);
    let mut ts = kova_native::text::TextSystem::new();
    let mut scene = Scene::new();
    let t = theme();
    let mut tree = ElementTree::new(
        move || showcase(state).into_any(),
        TextStyle::default(),
        t.background,
    );
    tree.set_viewport(Size::new(1120.0, 820.0), scale);
    let frame = tree.frame(&mut FrameContext {
        text: &mut ts,
        scene: &mut scene,
        atlas: renderer.atlas(),
        now: Instant::now(),
    });
    let (w, h) = ((1120.0 * scale) as u32, (820.0 * scale) as u32);
    let pixels = renderer.render_to_rgba(&gpu, &scene, w, h, t.background);
    image::save_buffer(path, &pixels, w, h, image::ColorType::Rgba8)
        .map_err(|e| KovaError::Other(e.to_string()))?;
    println!(
        "GPU capture: {w}x{h}; {:?}; {:?}",
        frame.stats,
        scene.stats()
    );
    Ok(())
}

#[cfg(test)]
mod tests;
