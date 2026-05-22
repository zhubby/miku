use axum::{Json, extract::State};
use miku_api::{AgentTurnRequest, AgentTurnResponse};

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "server.agent.run_turn", skip_all, fields(session_id = %request.session_id))]
pub(crate) async fn run_agent_turn(
    State(services): State<SharedServices>,
    Json(request): Json<AgentTurnRequest>,
) -> ServerResult<Json<AgentTurnResponse>> {
    Ok(Json(services.run_agent_turn(request).await?))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::router;
    use crate::test_support::DummyServices;

    #[tokio::test]
    async fn run_agent_turn_returns_agent_response() {
        let app = router(std::sync::Arc::new(DummyServices));
        let body = serde_json::json!({
            "session_id": "agent-1",
            "message": "hello",
            "context": {
                "cluster_id": "local",
                "cluster_name": "local",
                "selected_resource": null,
                "namespace": null
            },
            "history": []
        });

        let response = app
            .oneshot(
                Request::post("/api/agent/turn")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
