use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use miku_api::{
    ClusterConnectionInfo, ClusterInitializeRequest, ClusterStatusReport, ClusterStatusRequest,
    ClusterSummary, CreateClusterRequest, MikuServices, PodEvictRequest, PodLogQuery,
    ResourceApplyRequest, ResourceDeleteRequest, ResourceList, ResourceQuery, ResourceSummary,
};
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
        tracing::error!(status = %status, error = %self.0, "request failed");
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
        .route("/api/clusters", get(list_clusters).post(create_cluster))
        .route("/api/clusters/initialize", post(initialize_cluster))
        .route("/api/clusters/status", post(get_cluster_status))
        .route("/api/resources/list", post(list_resources))
        .route("/api/resources/apply", post(apply_resource))
        .route("/api/resources/delete", post(delete_resource))
        .route("/api/pods/evict", post(evict_pod))
        .route("/api/pods/logs", post(read_pod_logs))
        .route("/api/pods/logs/stream", post(stream_pod_logs))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(services)
}

pub async fn serve(
    bind: impl AsRef<str>,
    services: impl MikuServices + 'static,
) -> miku_core::Result<()> {
    let bind = bind.as_ref();
    let address: SocketAddr = bind.parse().map_err(|error: std::net::AddrParseError| {
        miku_core::MikuError::Config(error.to_string())
    })?;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
    tracing::info!(%address, "server listening");

    axum::serve(listener, router(Arc::new(services)))
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
}

#[tracing::instrument(name = "http.health")]
async fn health() -> Json<HealthPayload> {
    Json(HealthPayload {
        service: "miku-server",
        status: "ok",
    })
}

#[tracing::instrument(name = "http.list_clusters", skip(services))]
async fn list_clusters(
    State(services): State<SharedServices>,
) -> ServerResult<Json<Vec<ClusterSummary>>> {
    let clusters = services.list_clusters().await?;
    tracing::debug!(count = clusters.len(), "listed clusters");
    Ok(Json(clusters))
}

#[tracing::instrument(name = "http.create_cluster", skip(services, request), fields(context = %request.context))]
async fn create_cluster(
    State(services): State<SharedServices>,
    Json(request): Json<CreateClusterRequest>,
) -> ServerResult<Json<ClusterSummary>> {
    Ok(Json(services.create_cluster(request).await?))
}

#[tracing::instrument(name = "http.initialize_cluster", skip(services), fields(cluster_id = %request.cluster_id))]
async fn initialize_cluster(
    State(services): State<SharedServices>,
    Json(request): Json<ClusterInitializeRequest>,
) -> ServerResult<Json<ClusterConnectionInfo>> {
    Ok(Json(services.initialize_cluster(request).await?))
}

#[tracing::instrument(name = "http.get_cluster_status", skip(services), fields(cluster_id = %request.cluster_id))]
async fn get_cluster_status(
    State(services): State<SharedServices>,
    Json(request): Json<ClusterStatusRequest>,
) -> ServerResult<Json<ClusterStatusReport>> {
    Ok(Json(services.get_cluster_status(request).await?))
}

#[tracing::instrument(name = "http.list_resources", skip(services, query), fields(resource = %query.resource.plural))]
async fn list_resources(
    State(services): State<SharedServices>,
    Json(query): Json<ResourceQuery>,
) -> ServerResult<Json<ResourceList>> {
    let resources = services.list_resources(query).await?;
    tracing::debug!(count = resources.items.len(), "listed resources");
    Ok(Json(resources))
}

#[tracing::instrument(name = "http.apply_resource", skip(services, request), fields(resource = %request.resource.plural, name = %request.name))]
async fn apply_resource(
    State(services): State<SharedServices>,
    Json(request): Json<ResourceApplyRequest>,
) -> ServerResult<Json<ResourceSummary>> {
    Ok(Json(services.apply_resource(request).await?))
}

#[tracing::instrument(name = "http.delete_resource", skip(services, request), fields(resource = %request.resource.plural, name = %request.name))]
async fn delete_resource(
    State(services): State<SharedServices>,
    Json(request): Json<ResourceDeleteRequest>,
) -> ServerResult<StatusCode> {
    services.delete_resource(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "http.evict_pod", skip(services, request), fields(namespace = %request.namespace, pod = %request.pod))]
async fn evict_pod(
    State(services): State<SharedServices>,
    Json(request): Json<PodEvictRequest>,
) -> ServerResult<StatusCode> {
    services.evict_pod(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "http.read_pod_logs", skip(services, query), fields(namespace = %query.namespace, pod = %query.pod))]
async fn read_pod_logs(
    State(services): State<SharedServices>,
    Json(query): Json<PodLogQuery>,
) -> ServerResult<Json<Vec<miku_api::LogLine>>> {
    Ok(Json(services.read_logs(query).await?))
}

#[tracing::instrument(name = "http.stream_pod_logs", skip(services, query), fields(namespace = %query.namespace, pod = %query.pod))]
async fn stream_pod_logs(
    State(services): State<SharedServices>,
    Json(query): Json<PodLogQuery>,
) -> ServerResult<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let stream = services.stream_logs(query).await?.map(|result| {
        let event = match result {
            Ok(line) => Event::default()
                .json_data(line)
                .unwrap_or_else(|error| Event::default().event("error").data(error.to_string())),
            Err(error) => Event::default().event("error").data(error.to_string()),
        };
        Ok(event)
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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

    #[tokio::test]
    async fn resource_list_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resources/list")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ResourceQuery::new(
                            ClusterId::new("local"),
                            miku_core::ResourceRef::core("v1", "pods"),
                        ))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        assert_eq!(payload["items"][0]["name"], "api");
        assert_eq!(payload["items"][0]["kind"], "Pod");
    }

    #[tokio::test]
    async fn resource_apply_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resources/apply")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ResourceApplyRequest {
                            cluster_id: ClusterId::new("local"),
                            resource: miku_core::ResourceRef::core("v1", "pods"),
                            namespace: Some("default".to_owned()),
                            name: "api".to_owned(),
                            manifest: serde_json::json!({
                                "metadata": {"name": "api", "namespace": "default"}
                            }),
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
        assert_eq!(payload["name"], "api");
        assert_eq!(payload["namespace"], "default");
    }

    #[tokio::test]
    async fn resource_delete_route_returns_no_content() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resources/delete")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ResourceDeleteRequest {
                            cluster_id: ClusterId::new("local"),
                            resource: miku_core::ResourceRef::core("v1", "pods"),
                            namespace: Some("default".to_owned()),
                            name: "api".to_owned(),
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
    async fn pod_evict_route_returns_no_content() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/pods/evict")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&PodEvictRequest {
                            cluster_id: ClusterId::new("local"),
                            namespace: "default".to_owned(),
                            pod: "api".to_owned(),
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
    async fn pod_logs_route_serializes_log_lines() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/pods/logs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&PodLogQuery {
                            cluster_id: ClusterId::new("local"),
                            namespace: "default".to_owned(),
                            pod: "api".to_owned(),
                            container: Some("server".to_owned()),
                            tail_lines: Some(100),
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
        assert_eq!(payload[0]["text"], "api started");
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

        async fn create_cluster(
            &self,
            request: CreateClusterRequest,
        ) -> miku_core::Result<ClusterSummary> {
            Ok(ClusterSummary {
                id: ClusterId::new(request.context.clone()),
                name: request.context.clone(),
                context: request.context,
                current: false,
            })
        }
    }

    #[async_trait::async_trait]
    impl KubernetesResourceReader for DummyServices {
        async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
            assert_eq!(query.resource.plural, "pods");
            Ok(ResourceList {
                items: vec![ResourceSummary {
                    name: "api".to_owned(),
                    namespace: Some("default".to_owned()),
                    kind: "Pod".to_owned(),
                    status: Some("Running".to_owned()),
                    raw: serde_json::json!({}),
                }],
                continue_token: None,
            })
        }
    }

    #[async_trait::async_trait]
    impl ClusterInitializer for DummyServices {
        async fn initialize_cluster(
            &self,
            request: ClusterInitializeRequest,
        ) -> miku_core::Result<ClusterConnectionInfo> {
            assert_eq!(request.cluster_id, ClusterId::new("local"));
            Ok(ClusterConnectionInfo {
                version: "v1.35.0".to_owned(),
                platform: Some("darwin/arm64".to_owned()),
            })
        }
    }

    #[async_trait::async_trait]
    impl ClusterStatusReader for DummyServices {
        async fn get_cluster_status(
            &self,
            request: ClusterStatusRequest,
        ) -> miku_core::Result<ClusterStatusReport> {
            assert_eq!(request.cluster_id, ClusterId::new("local"));
            Ok(ClusterStatusReport {
                overview: ClusterStatusOverview {
                    version: "v1.35.0".to_owned(),
                    platform: Some("darwin/arm64".to_owned()),
                    namespaces: 2,
                    nodes: 1,
                    pods: 3,
                    ready_nodes: 1,
                    unhealthy_pods: 0,
                },
                conditions: vec![ClusterStatusCondition {
                    name: "Nodes".to_owned(),
                    status: "1/1 ready".to_owned(),
                    severity: ClusterStatusSeverity::Ok,
                    message: "All nodes are ready".to_owned(),
                }],
                workloads: ClusterStatusWorkloadSummary {
                    pods: 3,
                    deployments: 1,
                    services: 2,
                    config_maps: 4,
                    secrets: 5,
                },
                recent_events: vec![ClusterStatusEventSummary {
                    namespace: Some("default".to_owned()),
                    involved_object: "Pod/api".to_owned(),
                    reason: "Started".to_owned(),
                    message: "Started container api".to_owned(),
                    event_type: "Normal".to_owned(),
                }],
            })
        }
    }

    #[async_trait::async_trait]
    impl KubernetesResourceWriter for DummyServices {
        async fn apply_resource(
            &self,
            request: ResourceApplyRequest,
        ) -> miku_core::Result<ResourceSummary> {
            Ok(ResourceSummary {
                name: request.name,
                namespace: request.namespace,
                kind: "Pod".to_owned(),
                status: Some("Running".to_owned()),
                raw: request.manifest,
            })
        }

        async fn delete_resource(&self, _request: ResourceDeleteRequest) -> miku_core::Result<()> {
            Ok(())
        }

        async fn evict_pod(&self, _request: PodEvictRequest) -> miku_core::Result<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl KubernetesWatchService for DummyServices {}

    #[async_trait::async_trait]
    impl PodLogService for DummyServices {
        async fn read_logs(&self, _query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
            Ok(vec![LogLine {
                text: "api started".to_owned(),
            }])
        }
    }

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
