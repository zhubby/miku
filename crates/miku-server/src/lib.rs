use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use miku_api::{ClusterSummary, MikuServices};
use serde::Serialize;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

type SharedServices = Arc<dyn MikuServices>;
type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug)]
struct ServerError(miku_core::MikuError);

impl From<miku_core::MikuError> for ServerError {
    fn from(error: miku_core::MikuError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            miku_core::MikuError::NotFound(_) => StatusCode::NOT_FOUND,
            miku_core::MikuError::Config(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct HealthPayload {
    service: &'static str,
    status: &'static str,
}

pub fn router(services: SharedServices) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/clusters", get(list_clusters))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(services)
}

pub async fn serve(
    bind: impl AsRef<str>,
    services: impl MikuServices + 'static,
) -> miku_core::Result<()> {
    let address: SocketAddr =
        bind.as_ref()
            .parse()
            .map_err(|error: std::net::AddrParseError| {
                miku_core::MikuError::Config(error.to_string())
            })?;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

    axum::serve(listener, router(Arc::new(services)))
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
}

async fn health() -> Json<HealthPayload> {
    Json(HealthPayload {
        service: "miku-server",
        status: "ok",
    })
}

async fn list_clusters(
    State(services): State<SharedServices>,
) -> ServerResult<Json<Vec<ClusterSummary>>> {
    Ok(Json(services.list_clusters().await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use miku_api::*;
    use miku_core::ClusterId;
    use tower::ServiceExt;

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

    #[tokio::test]
    async fn cluster_route_serializes_trait_results() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/api/clusters")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(payload[0]["name"], "local");
    }

    struct DummyServices;

    #[async_trait::async_trait]
    impl ClusterRegistry for DummyServices {
        async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
            Ok(vec![ClusterSummary {
                id: ClusterId::new("local"),
                name: "local".to_owned(),
                context: "kind-miku".to_owned(),
                current: true,
            }])
        }
    }

    #[async_trait::async_trait]
    impl KubernetesResourceReader for DummyServices {
        async fn list_resources(&self, _query: ResourceQuery) -> miku_core::Result<ResourceList> {
            Ok(ResourceList::default())
        }
    }

    #[async_trait::async_trait]
    impl KubernetesWatchService for DummyServices {}

    #[async_trait::async_trait]
    impl PodLogService for DummyServices {}

    #[async_trait::async_trait]
    impl LocalPreferenceStore for DummyServices {
        async fn get_preference(&self, _key: &str) -> miku_core::Result<Option<serde_json::Value>> {
            Ok(None)
        }

        async fn set_preference(
            &self,
            _key: &str,
            _value: serde_json::Value,
        ) -> miku_core::Result<()> {
            Ok(())
        }
    }

    impl MikuServices for DummyServices {}
}
