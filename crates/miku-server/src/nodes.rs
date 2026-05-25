use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use miku_api::{NodeCordonRequest, NodeDrainRequest};

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "http.cordon_node", skip(services, request), fields(node = %request.node))]
pub(crate) async fn cordon_node(
    State(services): State<SharedServices>,
    Json(request): Json<NodeCordonRequest>,
) -> ServerResult<StatusCode> {
    services.cordon_node(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "http.drain_node", skip(services, request), fields(node = %request.node))]
pub(crate) async fn drain_node(
    State(services): State<SharedServices>,
    Json(request): Json<NodeDrainRequest>,
) -> ServerResult<StatusCode> {
    services.drain_node(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use miku_api::{NodeCordonRequest, NodeDrainRequest};
    use miku_core::ClusterId;
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

    #[tokio::test]
    async fn node_cordon_route_returns_no_content() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/cordon")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&NodeCordonRequest {
                            cluster_id: ClusterId::new("local"),
                            node: "worker-1".to_owned(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn node_drain_route_returns_no_content() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/drain")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&NodeDrainRequest {
                            cluster_id: ClusterId::new("local"),
                            node: "worker-1".to_owned(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
