use axum::Json;
use axum::extract::State;
use miku_api::{
    ClusterConnectionInfo, ClusterInitializeRequest, ClusterStatusReport, ClusterStatusRequest,
    ClusterSummary, CreateClusterRequest,
};

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "http.list_clusters", skip(services))]
pub(crate) async fn list_clusters(
    State(services): State<SharedServices>,
) -> ServerResult<Json<Vec<ClusterSummary>>> {
    let clusters = services.list_clusters().await?;
    tracing::debug!(count = clusters.len(), "listed clusters");
    Ok(Json(clusters))
}

#[tracing::instrument(name = "http.create_cluster", skip(services, request), fields(context = %request.context))]
pub(crate) async fn create_cluster(
    State(services): State<SharedServices>,
    Json(request): Json<CreateClusterRequest>,
) -> ServerResult<Json<ClusterSummary>> {
    Ok(Json(services.create_cluster(request).await?))
}

#[tracing::instrument(name = "http.initialize_cluster", skip(services), fields(cluster_id = %request.cluster_id))]
pub(crate) async fn initialize_cluster(
    State(services): State<SharedServices>,
    Json(request): Json<ClusterInitializeRequest>,
) -> ServerResult<Json<ClusterConnectionInfo>> {
    Ok(Json(services.initialize_cluster(request).await?))
}

#[tracing::instrument(name = "http.get_cluster_status", skip(services), fields(cluster_id = %request.cluster_id))]
pub(crate) async fn get_cluster_status(
    State(services): State<SharedServices>,
    Json(request): Json<ClusterStatusRequest>,
) -> ServerResult<Json<ClusterStatusReport>> {
    Ok(Json(services.get_cluster_status(request).await?))
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use miku_api::{ClusterInitializeRequest, ClusterStatusRequest};
    use miku_core::ClusterId;
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

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
        assert_eq!(payload[1]["id"], "miku-in-cluster");
        assert_eq!(payload[1]["name"], "In-cluster");
        assert_eq!(payload[1]["context"], "in-cluster");
        assert_eq!(payload[1]["current"], true);
    }

    #[tokio::test]
    async fn create_cluster_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "context": "kind-miku",
                            "config": "apiVersion: v1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(payload["context"], "kind-miku");
        assert!(payload.get("config").is_none());
    }

    #[tokio::test]
    async fn initialize_cluster_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters/initialize")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ClusterInitializeRequest {
                            cluster_id: ClusterId::new("local"),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(payload["version"], "v1.35.0");
        assert_eq!(payload["platform"], "darwin/arm64");
    }

    #[tokio::test]
    async fn cluster_status_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/clusters/status")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ClusterStatusRequest {
                            cluster_id: ClusterId::new("local"),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(payload["overview"]["version"], "v1.35.0");
        assert_eq!(payload["overview"]["ready_nodes"], 1);
        assert_eq!(payload["conditions"][0]["severity"], "Ok");
        assert_eq!(payload["workloads"]["deployments"], 1);
        assert_eq!(payload["recent_events"][0]["reason"], "Started");
    }
}
