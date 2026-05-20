use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::{Stream, StreamExt};
use miku_api::{PodEvictRequest, PodLogQuery};

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "http.evict_pod", skip(services, request), fields(namespace = %request.namespace, pod = %request.pod))]
pub(crate) async fn evict_pod(
    State(services): State<SharedServices>,
    Json(request): Json<PodEvictRequest>,
) -> ServerResult<StatusCode> {
    services.evict_pod(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "http.read_pod_logs", skip(services, query), fields(namespace = %query.namespace, pod = %query.pod))]
pub(crate) async fn read_pod_logs(
    State(services): State<SharedServices>,
    Json(query): Json<PodLogQuery>,
) -> ServerResult<Json<Vec<miku_api::LogLine>>> {
    Ok(Json(services.read_logs(query).await?))
}

#[tracing::instrument(name = "http.stream_pod_logs", skip(services, query), fields(namespace = %query.namespace, pod = %query.pod))]
pub(crate) async fn stream_pod_logs(
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
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use miku_api::{PodEvictRequest, PodLogQuery};
    use miku_core::ClusterId;
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

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
}
