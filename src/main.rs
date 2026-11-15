use mongo_studio::app::MongoStudioApp;
use mongo_studio::ui::app_icon::load_app_icon_data;
use mongo_studio::ui::layout::{
    DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH,
};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Mongo Studio")
        .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT]);

    if let Some(icon) = load_app_icon_data() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Mongo Studio",
        native_options,
        Box::new(|cc| Box::new(MongoStudioApp::new(cc))),
    )
}
