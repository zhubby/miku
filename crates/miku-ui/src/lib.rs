mod app;
#[cfg(not(target_arch = "wasm32"))]
mod cluster_events;
mod dialogs;
mod dock;
mod fonts;
mod forms;
mod menu;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod resource_panel;
mod resources;
mod state;
mod tabs;

pub use app::MikuApp;
pub use fonts::{install_fonts, install_icon_fonts};
#[cfg(not(target_arch = "wasm32"))]
pub use native::run_native_app;
pub use state::{AppState, RuntimeMode, app_title};
