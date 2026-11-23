//! jAyDHD-Grammar-Checker: a personal writing assistant. A small control panel switches features on and
//! off; each feature is a box (a window of its own) that floats anywhere on the screen.

// Release builds open no black console window behind the program; debug builds keep it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The analysis modules are WordCraft's, copied whole; the keyword and sentence-length helpers
// have no window yet.
#[allow(dead_code)]
mod analysis;
mod analysis_proof;
mod app;
mod capture;
#[allow(dead_code)]
mod dialogue;
mod doc_map;
mod docs;
mod inputwatch;
mod pets;
mod settings;
mod source;
mod thesaurus;
mod tray;
mod ui_kit;
mod win_shadow;

fn main() -> eframe::Result {
    win_shadow::start();
    let icon = ui_kit::icon_pixels(256).map(|(rgba, width, height)| eframe::egui::IconData {
        rgba,
        width,
        height,
    });
    let mut viewport = eframe::egui::ViewportBuilder::default()
            .with_inner_size([440.0, 760.0])
            // The floating boxes are see-through windows. egui makes one drawing surface for
            // all windows, and it only supports see-through ones when the first window asks.
            .with_transparent(true)
            .with_title("jAyDHD-Grammar-Checker");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        multisampling: 1,
        ..Default::default()
    };
    eframe::run_native(
        "jAyDHD-Grammar-Checker",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
