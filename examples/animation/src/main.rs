use kova::prelude::*;

fn main() -> KovaResult<()> {
    let owner = Owner::new_root();
    let moved = owner.with(|| signal(false));
    let app = Application::new()
        .title("Kova / Animation")
        .size(700.0, 450.0);
    let app = if std::env::args().any(|a| a == "--smoke") {
        app.run_for(1.0.secs())
    } else {
        app
    };
    let result = app.run(move || {
        column()
            .padding(32.0)
            .gap(24.0)
            .child(heading("Frame-rate independent motion"))
            .child(
                button("Retarget spring")
                    .self_start()
                    .on_click(move |_| moved.toggle()),
            )
            .child(
                div()
                    .size(64.0)
                    .rounded(18.0)
                    .bg(theme().accent)
                    .bind(move |s| {
                        if moved.get() {
                            s.translate_x(300.0).rotate(90.0)
                        } else {
                            s
                        }
                    })
                    .transition(Spring::bouncy()),
            )
            .child(spinner())
            .child(
                div()
                    .size(40.0)
                    .rounded(10.0)
                    .bg(theme().success)
                    .animation(
                        Animation::new(1.0.secs())
                            .repeat()
                            .alternate()
                            .easing(Easing::EaseInOutCubic),
                        |s, p| s.translate_x(300.0 * p).rotate(180.0 * p),
                    ),
            )
    });
    owner.dispose();
    let report = result?;
    assert!(report.presented_frames > 0);
    println!("Native run: {report:?}");
    Ok(())
}
