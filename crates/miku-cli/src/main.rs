use clap::Parser;
use miku_cli::{Cli, Command};

#[tokio::main]
async fn main() -> miku_core::Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);
    tracing::debug!(log_level = %cli.log_level, "tracing initialized");

    match cli.command_or_default() {
        Command::Gui(_) => {
            tracing::info!("starting native gui");
            let paths = cli
                .config_dir
                .map(miku_store::StorePaths::from_root)
                .map(Ok)
                .unwrap_or_else(miku_store::StorePaths::default_for_user)?;
            tracing::debug!(store_root = %paths.root().display(), "resolved store paths");
            let store = miku_store::SqliteStore::initialize(paths).await?;
            let services =
                match miku_kube::KubeServices::try_with_default_client(store.clone()).await {
                    Ok(services) => services,
                    Err(error) => {
                        tracing::warn!(%error, "starting gui without a live Kubernetes client");
                        miku_kube::KubeServices::new_offline(store)
                    }
                };
            miku_ui::run_native_app(
                std::sync::Arc::new(services),
                tokio::runtime::Handle::current(),
            )
            .map_err(|error| miku_core::MikuError::UnsupportedRuntime(error.to_string()))
        }
        Command::Server(server) => {
            tracing::info!(bind = %server.bind, "starting server command");
            let paths = cli
                .config_dir
                .map(miku_store::StorePaths::from_root)
                .map(Ok)
                .unwrap_or_else(miku_store::StorePaths::default_for_user)?;
            tracing::debug!(store_root = %paths.root().display(), "resolved store paths");
            let store = miku_store::SqliteStore::initialize(paths).await?;
            let services =
                match miku_kube::KubeServices::try_with_default_client(store.clone()).await {
                    Ok(services) => services,
                    Err(error) => {
                        tracing::warn!(%error, "starting server without a live Kubernetes client");
                        miku_kube::KubeServices::new_offline(store)
                    }
                };
            miku_server::serve(server.bind, services).await
        }
    }
}

fn init_tracing(log_level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(log_level).unwrap_or_else(|error| {
        eprintln!("invalid log level '{log_level}': {error}; falling back to info");
        tracing_subscriber::EnvFilter::new("info")
    });
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
