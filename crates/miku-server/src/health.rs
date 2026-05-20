use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct HealthPayload {
    service: &'static str,
    status: &'static str,
}

#[tracing::instrument(name = "http.health")]
pub(crate) async fn health() -> Json<HealthPayload> {
    Json(HealthPayload {
        service: "miku-server",
        status: "ok",
    })
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

    #[tokio::test]
    async fn health_route_reports_service_name() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"service": "miku-server", "status": "ok"})
        );
    }
}
