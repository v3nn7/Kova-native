use kova_native::prelude::*;

fn main() -> KovaResult<()> {
    let app = Application::new()
        .title("Kova Native / Hello")
        .size(640.0, 420.0);
    let app = if std::env::args().any(|a| a == "--smoke") {
        app.run_for(1.0.secs())
    } else {
        app
    };
    let report = app.run(|| {
        column()
            .center()
            .gap(12.0)
            .child(heading("Hello, Kova Native."))
            .child(label("A native window. Rust elements. GPU pixels."))
    })?;
    assert!(report.presented_frames > 0);
    println!("Native run: {report:?}");
    Ok(())
}
