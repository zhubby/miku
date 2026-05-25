use axum::{
    Json,
    extract::{Path, State},
};
use miku_api::{
    AgentConversation, AgentConversationSummary, AgentPersistedMessage, AgentTurnRequest,
    AgentTurnResponse, AppendAgentMessageRequest, CreateAgentConversationRequest,
};

use crate::SharedServices;
use crate::error::ServerResult;

#[tracing::instrument(name = "server.agent.run_turn", skip_all, fields(session_id = %request.session_id))]
pub(crate) async fn run_agent_turn(
    State(services): State<SharedServices>,
    Json(request): Json<AgentTurnRequest>,
) -> ServerResult<Json<AgentTurnResponse>> {
    Ok(Json(services.run_agent_turn(request).await?))
}

pub(crate) async fn list_agent_conversations(
    State(services): State<SharedServices>,
) -> ServerResult<Json<Vec<AgentConversationSummary>>> {
    Ok(Json(services.list_agent_conversations().await?))
}

pub(crate) async fn get_agent_conversation(
    State(services): State<SharedServices>,
    Path(id): Path<String>,
) -> ServerResult<Json<AgentConversation>> {
    let conversation = services
        .get_agent_conversation(&id)
        .await?
        .ok_or_else(|| miku_core::MikuError::NotFound(format!("agent conversation '{id}'")))?;
    Ok(Json(conversation))
}

pub(crate) async fn create_agent_conversation(
    State(services): State<SharedServices>,
    Json(request): Json<CreateAgentConversationRequest>,
) -> ServerResult<Json<AgentConversationSummary>> {
    Ok(Json(services.create_agent_conversation(request).await?))
}

pub(crate) async fn append_agent_message(
    State(services): State<SharedServices>,
    Path(id): Path<String>,
    Json(mut request): Json<AppendAgentMessageRequest>,
) -> ServerResult<Json<AgentPersistedMessage>> {
    request.conversation_id = id;
    Ok(Json(services.append_agent_message(request).await?))
}

pub(crate) async fn delete_agent_conversation(
    State(services): State<SharedServices>,
    Path(id): Path<String>,
) -> ServerResult<()> {
    services.delete_agent_conversation(&id).await?;
    Ok(())
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

    #[tokio::test]
    async fn list_agent_conversations_returns_summaries() {
        let app = router(std::sync::Arc::new(DummyServices));

        let response = app
            .oneshot(
                Request::get("/api/agent/conversations")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_agent_conversation_returns_not_found() {
        let app = router(std::sync::Arc::new(DummyServices));

        let response = app
            .oneshot(
                Request::get("/api/agent/conversations/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
