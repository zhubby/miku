use std::sync::Arc;

use eframe::egui;
use miku_api::MikuServices;

use crate::app::MikuApp;
use crate::fonts::install_fonts;
use crate::state::{RuntimeMode, app_title};

pub(crate) fn read_config_file(path: &std::path::Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

pub fn run_native_app(
    services: Arc<dyn MikuServices>,
    runtime: tokio::runtime::Handle,
) -> eframe::Result {
    tracing::info!("launching native egui app");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        app_title(RuntimeMode::Native),
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(MikuApp::native(services.clone(), runtime.clone())))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_config_file_returns_file_contents() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "apiVersion: v1").unwrap();

        let content = read_config_file(temp.path()).unwrap();

        assert_eq!(content, "apiVersion: v1");
    }
}
