use std::error::Error;

use eframe::egui;

use stemma_flow::app::StemmaApp;
use stemma_flow::gui::window::TreeWindow;

fn main() -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Stemma Flow")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stemma Flow",
        options,
        Box::new(|cc| {
            let app = StemmaApp::new();
            Ok(Box::new(TreeWindow::new(app, cc.storage)))
        }),
    )?;
    Ok(())
}
