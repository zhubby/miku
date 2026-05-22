use axum::Json;
use axum::extract::State;
use miku_api::LlmProviderSettings;

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "http.get_llm_settings", skip(services))]
pub(crate) async fn get_llm_settings(
    State(services): State<SharedServices>,
) -> ServerResult<Json<LlmProviderSettings>> {
    Ok(Json(services.get_llm_settings().await?))
}

#[tracing::instrument(name = "http.set_llm_settings", skip(services, settings))]
pub(crate) async fn set_llm_settings(
    State(services): State<SharedServices>,
    Json(settings): Json<LlmProviderSettings>,
) -> ServerResult<()> {
    services.set_llm_settings(settings).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

    #[tokio::test]
    async fn llm_settings_routes_round_trip_trait_result() {
        let services = std::sync::Arc::new(DummyServices);
        let request = miku_api::LlmProviderSettings {
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: "sk-test".to_owned(),
            model: "gpt-5.1".to_owned(),
            stream: true,
        };

        let response = router(services.clone())
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/settings/llm")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router(services)
            .oneshot(
                Request::builder()
                    .uri("/api/settings/llm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<miku_api::LlmProviderSettings>(&body).unwrap();
        assert_eq!(payload, request);
    }
}
