use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::{Stream, StreamExt};
use miku_api::{
    ResourceApplyRequest, ResourceDeleteRequest, ResourceEvent, ResourceList, ResourcePatchRequest,
    ResourceQuery, ResourceSummary,
};
use serde::Deserialize;

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "http.list_resources", skip(services, query), fields(resource = %query.resource.plural))]
pub(crate) async fn list_resources(
    State(services): State<SharedServices>,
    Json(query): Json<ResourceQuery>,
) -> ServerResult<Json<ResourceList>> {
    let resources = services.list_resources(query).await?;
    tracing::debug!(count = resources.items.len(), "listed resources");
    Ok(Json(resources))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResourceWatchParams {
    cluster_id: String,
    group: Option<String>,
    version: String,
    plural: String,
    namespace: Option<String>,
    label_selector: Option<String>,
    limit: Option<u32>,
}

impl ResourceWatchParams {
    fn into_query(self) -> ResourceQuery {
        let resource = match self.group {
            Some(group) if !group.is_empty() => {
                miku_core::ResourceRef::grouped(group, self.version, self.plural)
            }
            _ => miku_core::ResourceRef::core(self.version, self.plural),
        };
        let mut query = ResourceQuery::new(miku_core::ClusterId::new(self.cluster_id), resource);
        query.namespace = self.namespace.filter(|namespace| !namespace.is_empty());
        query.label_selector = self
            .label_selector
            .filter(|selector| !selector.trim().is_empty());
        if let Some(limit) = self.limit {
            query.limit = Some(limit);
        }
        query
    }
}

#[tracing::instrument(name = "http.watch_resources", skip(services), fields(resource = %params.plural))]
pub(crate) async fn watch_resources(
    State(services): State<SharedServices>,
    Query(params): Query<ResourceWatchParams>,
) -> ServerResult<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let stream = services
        .watch_resources(params.into_query())
        .await?
        .map(|result| {
            let event = match result {
                Ok(event) => resource_sse_event(event),
                Err(error) => Event::default().event("error").data(error.to_string()),
            };
            Ok(event)
        });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn resource_sse_event(event: ResourceEvent) -> Event {
    Event::default()
        .json_data(event)
        .unwrap_or_else(|error| Event::default().event("error").data(error.to_string()))
}

#[tracing::instrument(name = "http.apply_resource", skip(services, request), fields(resource = %request.resource.plural, name = %request.name))]
pub(crate) async fn apply_resource(
    State(services): State<SharedServices>,
    Json(request): Json<ResourceApplyRequest>,
) -> ServerResult<Json<ResourceSummary>> {
    Ok(Json(services.apply_resource(request).await?))
}

#[tracing::instrument(name = "http.patch_resource", skip(services, request), fields(resource = %request.resource.plural, name = %request.name))]
pub(crate) async fn patch_resource(
    State(services): State<SharedServices>,
    Json(request): Json<ResourcePatchRequest>,
) -> ServerResult<Json<ResourceSummary>> {
    Ok(Json(services.patch_resource(request).await?))
}

#[tracing::instrument(name = "http.delete_resource", skip(services, request), fields(resource = %request.resource.plural, name = %request.name))]
pub(crate) async fn delete_resource(
    State(services): State<SharedServices>,
    Json(request): Json<ResourceDeleteRequest>,
) -> ServerResult<StatusCode> {
    services.delete_resource(request).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use miku_api::{
        ResourceApplyRequest, ResourceDeleteRequest, ResourcePatchRequest, ResourceQuery,
    };
    use miku_core::ClusterId;
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

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
    async fn resource_watch_route_serializes_snapshot_events() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .uri("/api/resources/watch?cluster_id=local&version=v1&plural=pods&namespace=default&limit=25")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload = String::from_utf8(body.to_vec()).unwrap();
        assert!(payload.contains("data:"));
        assert!(payload.contains("\"Snapshot\""));
        assert!(payload.contains("\"api\""));
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
    async fn resource_patch_route_serializes_trait_result() {
        let response = router(std::sync::Arc::new(DummyServices))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resources/patch")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&ResourcePatchRequest {
                            cluster_id: ClusterId::new("local"),
                            resource: miku_core::ResourceRef::grouped("apps", "v1", "deployments"),
                            namespace: Some("default".to_owned()),
                            name: "api".to_owned(),
                            patch: serde_json::json!({"spec": {"replicas": 2}}),
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
        assert_eq!(payload["raw"]["spec"]["replicas"], 2);
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
}
