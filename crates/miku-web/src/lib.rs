#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
#[wasm_bindgen]
pub struct WebHandle {
    runner: eframe::WebRunner,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WebHandle {
    #[expect(clippy::new_without_default)]
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    #[wasm_bindgen]
    pub async fn start(&self, canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
        tracing::info!("starting web app");
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    miku_ui::install_fonts(&cc.egui_ctx);
                    let app = web_app().unwrap_or_else(|error| {
                        tracing::warn!(%error, "starting web app without HTTP services");
                        miku_ui::MikuApp::web()
                    });
                    Ok(Box::new(app))
                }),
            )
            .await
    }

    #[wasm_bindgen]
    pub fn destroy(&self) {
        tracing::info!("destroying web app");
        self.runner.destroy();
    }

    #[wasm_bindgen]
    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window is not available"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("document is not available"))?;
    let canvas = document
        .get_element_by_id("miku-canvas")
        .ok_or_else(|| JsValue::from_str("miku-canvas element is not available"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let handle = WebHandle::new();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = handle.start(canvas).await {
            tracing::error!(?error, "failed to start web app");
        }
    });

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn web_app() -> Result<miku_ui::MikuApp, String> {
    let window = web_sys::window().ok_or_else(|| "browser window is not available".to_owned())?;
    let origin = window
        .location()
        .origin()
        .map_err(|error| format!("failed to read browser origin: {error:?}"))?;
    let client =
        miku_http_client::HttpMikuClient::new(&origin).map_err(|error| error.to_string())?;
    Ok(miku_ui::MikuApp::web_with_services(std::sync::Arc::new(
        client,
    )))
}
