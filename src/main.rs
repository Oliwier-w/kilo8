use kilo8::ui::Kilo8App;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Kilo8")
            .with_inner_size([900.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kilo8",
        native_options,
        Box::new(|cc| Ok(Box::new(Kilo8App::new(cc)))),
    )
}
