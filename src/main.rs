use std::error::Error;

use eframe::egui;

use stemma_flow::app::StemmaApp;
use stemma_flow::db;
use stemma_flow::gui::window::TreeWindow;

const DB_PATH: &str = "stemma-flow.db";

fn main() -> Result<(), Box<dyn Error>> {
    let conn = db::open_connection(DB_PATH)?;
    db::init_db(&conn)?;
    drop(conn);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Stemma Flow")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stemma Flow",
        options,
        Box::new(|_cc| Ok(Box::new(TreeWindow::new(StemmaApp::new(DB_PATH))))),
    )?;
    Ok(())
}
