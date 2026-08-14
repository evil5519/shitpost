//! egui/eframe adapter for the portfolio application.

#![warn(clippy::all, rust_2018_idioms)]

pub use eframe;

/// Loads the framework-independent session snapshot from eframe storage.
#[must_use]
pub fn load_session(storage: &dyn eframe::Storage) -> core::SessionSnapshot {
    let snapshot =
        eframe::get_value(storage, eframe::APP_KEY).unwrap_or_else(|| core::SessionSnapshot {
            schema_version: core::CURRENT_SCHEMA_VERSION,
            ..core::SessionSnapshot::default()
        });
    core::migrate(snapshot).0
}

/// Saves a framework-independent session snapshot through eframe storage.
pub fn save_session(storage: &mut dyn eframe::Storage, snapshot: &core::SessionSnapshot) {
    eframe::set_value(storage, eframe::APP_KEY, snapshot);
}

mod app;
pub use app::PortfolioApp;

/// Native launcher result type.
pub type AppResult = eframe::Result;

/// Runs the native eframe application.
#[cfg(not(target_arch = "wasm32"))]
/// # Errors
/// Returns an eframe startup error when the native application cannot launch.
pub fn run_native() -> AppResult {
    env_logger::init();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([360.0, 480.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(
                    &include_bytes!("../../../assets/favicon-512x512.png")[..],
                )
                .expect("Failed to load icon"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Portfolio",
        native_options,
        Box::new(|cc| Ok(Box::new(PortfolioApp::new(cc)))),
    )
}

/// Starts the WASM eframe application and manages the HTML loading indicator.
#[cfg(target_arch = "wasm32")]
pub fn run_web() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();
    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async move {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");
        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(PortfolioApp::new(cc)))),
            )
            .await;
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(()) => loading_text.remove(),
                Err(err) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {err:?}");
                }
            }
        }
    });
}
