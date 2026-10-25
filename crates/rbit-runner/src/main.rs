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

mod backend;
mod check;
mod cli;
mod client;
mod commands;
mod config;
mod contexts;
mod envfiles;
mod job;
mod logs;
mod mask;
mod poller;
mod process;
mod register;
mod steps;
mod task;
#[cfg(test)]
mod testing;

use std::{io::IsTerminal as _, path::Path, sync::Arc};

use clap::Parser as _;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::{
    cli::{Cli, Command},
    client::Client,
    job::JobExecutor,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging();
    client::install_crypto_provider();
    match cli.command {
        Command::Register(args) => register::run(&cli.config, args).await,
        Command::Check => check::run(&cli.config).await,
        Command::Run { once } => run(&cli.config, once).await,
        Command::Exec(_) => anyhow::bail!("`rbit-runner exec` is not available in this build yet"),
    }
}

/// `rbit-runner run`: polls until Ctrl-C or SIGTERM, then lets running tasks finish.
async fn run(config_path: &Path, once: bool) -> anyhow::Result<()> {
    let loaded = config::load(config_path)?;
    let credential = config::read_credential(&loaded.credential_path)?;
    if let Some(warning) = config::host_warning(&loaded.config) {
        tracing::warn!("{warning}");
    }
    let client = Client::new(&config::server_url(&loaded.config.url, loaded.config.insecure_http)?, Some(credential))?;
    if loaded.config.backends().contains(&rbit_workflow::protocol::Backend::Container) {
        backend::container::janitor(&loaded.config).await;
    }
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("stopping: no new tasks are taken");
        signal.cancel();
    });
    let executor = JobExecutor::new(Arc::new(loaded.clone()));
    poller::Poller::new(client, loaded, Arc::new(executor), once).run(shutdown).await
}

/// Logs to standard error; `RUST_LOG` overrides the level.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_ansi(ansi).with_writer(std::io::stderr).compact().init();
}
