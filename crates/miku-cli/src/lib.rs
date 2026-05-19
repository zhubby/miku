use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

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
}
