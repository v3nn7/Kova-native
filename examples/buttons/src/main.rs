use kova::prelude::*;

fn main() -> KovaResult<()> {
    let owner = Owner::new_root();
    let clicks = owner.with(|| signal(0));
    let app = Application::new()
        .title("Kova / Buttons")
        .size(760.0, 440.0);
    let app = if std::env::args().any(|a| a == "--smoke") {
        app.run_for(1.0.secs())
    } else {
        app
    };
    let result = app.run(move || {
        column()
            .center()
            .gap(20.0)
            .child(heading("The existing button family"))
            .child(
                row()
                    .gap(10.0)
                    .child(button("Primary").on_click(move |_| clicks.update(|n| *n += 1)))
                    .child(button("Secondary").secondary())
                    .child(button("Outline").outline())
                    .child(button("Ghost").ghost())
                    .child(button("Danger").danger()),
            )
            .child(button("Disabled").disabled(true))
            .child(text(move || format!("Clicked {} times", clicks.get())))
    });
    owner.dispose();
    let report = result?;
    assert!(report.presented_frames > 0);
    println!("Native run: {report:?}");
    Ok(())
}
