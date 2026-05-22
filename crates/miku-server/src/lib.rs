use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use miku_api::MikuServices;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

mod agent;
mod clusters;
mod error;
mod health;
mod pods;
mod resources;
mod static_assets;

#[cfg(test)]
mod test_support;

type SharedServices = Arc<dyn MikuServices>;

pub fn router(services: SharedServices) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route(
            "/api/clusters",
            get(clusters::list_clusters).post(clusters::create_cluster),
        )
        .route(
            "/api/clusters/initialize",
            post(clusters::initialize_cluster),
        )
        .route("/api/clusters/status", post(clusters::get_cluster_status))
        .route("/api/agent/turn", post(agent::run_agent_turn))
        .route("/api/resources/list", post(resources::list_resources))
        .route("/api/resources/watch", get(resources::watch_resources))
        .route("/api/resources/apply", post(resources::apply_resource))
        .route("/api/resources/delete", post(resources::delete_resource))
        .route("/api/pods/evict", post(pods::evict_pod))
        .route("/api/pods/attach", get(pods::attach_pod))
        .route("/api/pods/logs", post(pods::read_pod_logs))
        .route("/api/pods/logs/stream", post(pods::stream_pod_logs))
        .fallback(static_assets::serve)
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
