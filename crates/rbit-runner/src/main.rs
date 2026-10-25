//! `rbit-runner` — runs rbit Actions jobs (§12.5 of `docs/design/actions.md`).
//!
//! ```text
//! rbit-runner register --url <rbit> --token <registration token> [--name] [--labels]
//! rbit-runner run [--once]          # poll for tasks and run them
//! rbit-runner exec <workflow.yml>   # run a workflow from a local checkout, no server
//! rbit-runner check                 # docker/podman, git and shells on this host
//! ```
//!
//! The runner speaks protocol v1 to rbit over HTTPS and depends on no server crate: the
//! wire contract is `rbit-workflow::{protocol, spec}`. Jobs run in Docker or Podman
//! containers, or, for labels configured so, directly on the host.

mod check;
mod cli;
mod client;
mod config;
mod process;
mod register;

use std::io::IsTerminal as _;

use clap::Parser as _;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging();
    client::install_crypto_provider();
    match cli.command {
        Command::Register(args) => register::run(&cli.config, args).await,
        Command::Check => check::run(&cli.config).await,
        Command::Run { .. } => {
            let loaded = config::load(&cli.config)?;
            config::read_credential(&loaded.credential_path)?;
            if let Some(warning) = config::host_warning(&loaded.config) {
                tracing::warn!("{warning}");
            }
            anyhow::bail!("`rbit-runner run` cannot run tasks in this build yet (work directory {})", loaded.work_dir.display())
        }
        Command::Exec(_) => anyhow::bail!("`rbit-runner exec` is not available in this build yet"),
    }
}

/// Logs to standard error; `RUST_LOG` overrides the level.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_ansi(ansi).with_writer(std::io::stderr).compact().init();
}
