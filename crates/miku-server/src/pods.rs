use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, RawQuery, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::{Stream, StreamExt};
use miku_api::{
    PodAttachInput, PodAttachOutput, PodAttachRequest, PodEvictRequest, PodExecRequest,
    PodLogQuery, default_pod_exec_command,
};
use serde::Deserialize;
use url::form_urlencoded;

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

#[derive(Debug, Deserialize)]
pub(crate) struct PodAttachQuery {
    cluster_id: String,
    namespace: String,
    pod: String,
    container: Option<String>,
    tty: Option<bool>,
}

pub(crate) async fn attach_pod(
    State(services): State<SharedServices>,
    Query(query): Query<PodAttachQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| async move {
        let request = PodAttachRequest {
            cluster_id: miku_core::ClusterId::new(query.cluster_id),
            namespace: query.namespace,
            pod: query.pod,
            container: query.container,
            tty: query.tty.unwrap_or(true),
        };
        handle_terminal_socket(socket, services, PodTerminalRequest::Attach(request)).await;
    })
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PodExecQuery {
    cluster_id: String,
    namespace: String,
    pod: String,
    container: Option<String>,
    command: Vec<String>,
    tty: Option<bool>,
}

impl PodExecQuery {
    fn from_raw_query(raw_query: Option<&str>) -> Result<Self, String> {
        let mut cluster_id = None;
        let mut namespace = None;
        let mut pod = None;
        let mut container = None;
        let mut command = Vec::new();
        let mut tty = None;

        if let Some(raw_query) = raw_query {
            for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
                match key.as_ref() {
                    "cluster_id" => cluster_id = Some(value.into_owned()),
                    "namespace" => namespace = Some(value.into_owned()),
                    "pod" => pod = Some(value.into_owned()),
                    "container" => container = Some(value.into_owned()),
                    "command" => command.push(value.into_owned()),
                    "tty" => tty = Some(parse_bool_query_param("tty", &value)?),
                    _ => {}
                }
            }
        }

        Ok(Self {
            cluster_id: required_query_param(cluster_id, "cluster_id")?,
            namespace: required_query_param(namespace, "namespace")?,
            pod: required_query_param(pod, "pod")?,
            container,
            command,
            tty,
        })
    }

    fn into_request(self) -> PodExecRequest {
        PodExecRequest {
            cluster_id: miku_core::ClusterId::new(self.cluster_id),
            namespace: self.namespace,
            pod: self.pod,
            container: self.container,
            command: if self.command.is_empty() {
                default_pod_exec_command()
            } else {
                self.command
            },
            tty: self.tty.unwrap_or(true),
        }
    }
}

pub(crate) async fn exec_pod(
    State(services): State<SharedServices>,
    RawQuery(raw_query): RawQuery,
    upgrade: WebSocketUpgrade,
) -> Response {
    let query = match PodExecQuery::from_raw_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
    };

    upgrade.on_upgrade(move |socket| async move {
        handle_terminal_socket(
            socket,
            services,
            PodTerminalRequest::Exec(query.into_request()),
        )
        .await;
    })
}

fn required_query_param(value: Option<String>, name: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("missing required query parameter `{name}`"))
}

fn parse_bool_query_param(name: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("invalid boolean query parameter `{name}`")),
    }
}

enum PodTerminalRequest {
    Attach(PodAttachRequest),
    Exec(PodExecRequest),
}

async fn handle_terminal_socket(
    mut socket: WebSocket,
    services: SharedServices,
    request: PodTerminalRequest,
) {
    let session = match request {
        PodTerminalRequest::Attach(request) => services.attach_pod(request).await,
        PodTerminalRequest::Exec(request) => services.exec_pod(request).await,
    };
    let Ok(mut session) = session else {
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&PodAttachOutput::Closed)
                    .unwrap_or_else(|_| "{\"Closed\":null}".to_owned())
                    .into(),
            ))
            .await;
        return;
    };

    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else {
                    let _ = session.input.unbounded_send(PodAttachInput::Close);
                    break;
                };
                match message {
                    Message::Binary(bytes) => {
                        let _ = session.input.unbounded_send(PodAttachInput::Bytes(bytes.to_vec()));
                    }
                    Message::Text(text) => {
                        match serde_json::from_str::<PodAttachInput>(&text) {
                            Ok(input) => {
                                let close = matches!(input, PodAttachInput::Close);
                                let _ = session.input.unbounded_send(input);
                                if close {
                                    break;
                                }
                            }
                            Err(error) => {
                                let output = PodAttachOutput::Stderr(error.to_string().into_bytes());
                                if send_attach_output(&mut socket, output).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        let _ = session.input.unbounded_send(PodAttachInput::Close);
                        break;
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            output = session.output.next() => {
                let Some(output) = output else {
                    let _ = send_attach_output(&mut socket, PodAttachOutput::Closed).await;
                    break;
                };
                let output = output.unwrap_or_else(|error| PodAttachOutput::Stderr(error.to_string().into_bytes()));
                let close = matches!(output, PodAttachOutput::Closed);
                if send_attach_output(&mut socket, output).await.is_err() || close {
                    break;
                }
            }
        }
    }
}

async fn send_attach_output(
    socket: &mut WebSocket,
    output: PodAttachOutput,
) -> Result<(), axum::Error> {
    match output {
        PodAttachOutput::Stdout(bytes) => socket.send(Message::Binary(bytes.into())).await,
        PodAttachOutput::Stderr(_) | PodAttachOutput::Closed => {
            socket
                .send(Message::Text(
                    serde_json::to_string(&output)
                        .unwrap_or_else(|_| "{\"Closed\":null}".to_owned())
                        .into(),
                ))
                .await
        }
    }
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
    use miku_api::{PodEvictRequest, PodLogQuery, default_pod_exec_command};
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

    #[test]
    fn pod_exec_query_parses_repeated_command_params() {
        let query = super::PodExecQuery::from_raw_query(Some(
            "cluster_id=local&namespace=default&pod=api&container=server&command=%2Fbin%2Fsh&command=-lc&command=echo+ready&tty=true",
        ))
        .unwrap();

        assert_eq!(query.cluster_id, "local");
        assert_eq!(query.namespace, "default");
        assert_eq!(query.pod, "api");
        assert_eq!(query.container.as_deref(), Some("server"));
        assert_eq!(query.command, ["/bin/sh", "-lc", "echo ready"]);
        assert_eq!(query.tty, Some(true));
    }

    #[test]
    fn pod_exec_query_defaults_command_and_tty_later() {
        let query =
            super::PodExecQuery::from_raw_query(Some("cluster_id=local&namespace=default&pod=api"))
                .unwrap();
        let request = query.into_request();

        assert_eq!(request.command, default_pod_exec_command());
        assert!(request.tty);
    }
}
