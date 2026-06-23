use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use std::future::Future;

#[derive(Debug, Parser)]
#[command(name = "miku", about = "Kubernetes management UI")]
pub struct Cli {
    #[arg(long, global = true, env = "MIKU_CONFIG_DIR")]
    pub config_dir: Option<PathBuf>,

    #[arg(long, global = true, default_value = "info", env = "MIKU_LOG_LEVEL")]
    pub log_level: String,

    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn command_or_default(&self) -> Command {
        self.command.clone().unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    Gui(GuiArgs),
    Server(ServerArgs),
}

impl Default for Command {
    fn default() -> Self {
        Self::Gui(GuiArgs {})
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct GuiArgs {}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct ServerArgs {
    #[arg(long, default_value = "127.0.0.1:5174")]
    pub bind: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerServiceSource {
    InClusterServiceAccount,
    DefaultClient,
    Offline,
}

pub async fn server_services(
    store: miku_store::SqliteStore,
) -> (
    miku_kube::KubeServices<miku_store::SqliteStore>,
    ServerServiceSource,
) {
    choose_server_services(
        store,
        |store| async move { miku_kube::KubeServices::try_with_incluster_service_account(store).await },
        |store| async move { miku_kube::KubeServices::try_with_default_client(store).await },
    )
    .await
}

async fn choose_server_services<S, InCluster, InClusterFuture, Default, DefaultFuture>(
    store: S,
    in_cluster: InCluster,
    default_client: Default,
) -> (miku_kube::KubeServices<S>, ServerServiceSource)
where
    S: Clone,
    InCluster: FnOnce(S) -> InClusterFuture,
    InClusterFuture: Future<Output = miku_core::Result<miku_kube::KubeServices<S>>>,
    Default: FnOnce(S) -> DefaultFuture,
    DefaultFuture: Future<Output = miku_core::Result<miku_kube::KubeServices<S>>>,
{
    match in_cluster(store.clone()).await {
        Ok(services) => {
            tracing::info!("starting server with in-cluster Kubernetes service account");
            (services, ServerServiceSource::InClusterServiceAccount)
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "in-cluster Kubernetes service account unavailable; trying default Kubernetes client"
            );
            match default_client(store.clone()).await {
                Ok(services) => {
                    tracing::info!("starting server with default Kubernetes client");
                    (services, ServerServiceSource::DefaultClient)
                }
                Err(default_error) => {
                    tracing::warn!(
                        %default_error,
                        "starting server without a live Kubernetes client"
                    );
                    (
                        miku_kube::KubeServices::new_offline(store),
                        ServerServiceSource::Offline,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn no_subcommand_defaults_to_gui() {
        let cli = Cli::parse_from(["miku"]);

        assert!(matches!(cli.command_or_default(), Command::Gui(_)));
    }

    #[test]
    fn server_subcommand_accepts_bind_address() {
        let cli = Cli::parse_from(["miku", "server", "--bind", "127.0.0.1:5174"]);

        match cli.command_or_default() {
            Command::Server(server) => assert_eq!(server.bind, "127.0.0.1:5174"),
            Command::Gui(_) => panic!("expected server command"),
        }
    }

    #[test]
    fn log_level_can_be_configured_globally() {
        let cli = Cli::parse_from(["miku", "--log-level", "debug", "server"]);

        assert_eq!(cli.log_level, "debug");
    }

    #[tokio::test]
    async fn server_services_prefers_incluster_service_account() {
        let default_called = Arc::new(AtomicBool::new(false));
        let default_called_for_closure = default_called.clone();

        let (_services, source) = choose_server_services(
            (),
            |store| async move { Ok(miku_kube::KubeServices::new_offline(store)) },
            move |store| {
                let default_called = default_called_for_closure.clone();
                async move {
                    default_called.store(true, Ordering::Relaxed);
                    Ok(miku_kube::KubeServices::new_offline(store))
                }
            },
        )
        .await;

        assert_eq!(source, ServerServiceSource::InClusterServiceAccount);
        assert!(!default_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn server_services_falls_back_to_default_client() {
        let (_services, source) = choose_server_services(
            (),
            |_store| async move {
                Err(miku_core::MikuError::Kubernetes(
                    "service account unavailable".to_owned(),
                ))
            },
            |store| async move { Ok(miku_kube::KubeServices::new_offline(store)) },
        )
        .await;

        assert_eq!(source, ServerServiceSource::DefaultClient);
    }

    #[tokio::test]
    async fn server_services_falls_back_to_offline() {
        let (services, source) = choose_server_services(
            (),
            |_store| async move {
                Err(miku_core::MikuError::Kubernetes(
                    "service account unavailable".to_owned(),
                ))
            },
            |_store| async move {
                Err(miku_core::MikuError::Kubernetes(
                    "default client unavailable".to_owned(),
                ))
            },
        )
        .await;

        assert_eq!(source, ServerServiceSource::Offline);
        assert!(!services.has_live_client());
    }
}
