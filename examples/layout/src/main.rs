use kova_native::prelude::*;

fn main() -> KovaResult<()> {
    let app = Application::new()
        .title("Kova Native / Layout")
        .size(780.0, 520.0);
    let app = if std::env::args().any(|a| a == "--smoke") {
        app.run_for(1.0.secs())
    } else {
        app
    };
    let report = app.run(|| {
        column()
            .padding(28.0)
            .gap(18.0)
            .child(heading("Flex, grid and stack already work."))
            .child(
                row()
                    .gap(12.0)
                    .child(card().flex_1().child("Flex / 1"))
                    .child(card().flex_1().child("Flex / 1")),
            )
            .child(div().grid_cols(3).gap(12.0).children((0..6).map(|i| {
                card()
                    .h(78.0)
                    .center()
                    .child(text(format!("Grid cell {}", i + 1)))
            })))
            .child(
                stack()
                    .h(80.0)
                    .child(div().bg(theme().accent_soft).rounded(12.0))
                    .child(text("Stack / shared grid cell").self_center().mx_auto()),
            )
    })?;
    assert!(report.presented_frames > 0);
    println!("Native run: {report:?}");
    Ok(())
}
