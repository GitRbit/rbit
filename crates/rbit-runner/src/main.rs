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
mod exec;
mod integrity;
mod job;
mod logs;
mod mask;
mod node;
mod poller;
mod process;
mod register;
mod shutdown;
mod steps;
mod task;
#[cfg(test)]
mod testing;
mod untrusted;

use std::{io::IsTerminal as _, path::Path, sync::Arc};

use anyhow::Context as _;
use clap::Parser as _;
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
        Command::Exec(args) => exec::run(&cli.config, args).await,
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
    task::prepare_tasks_dir(&loaded.work_dir).with_context(|| format!("cannot create {}", loaded.work_dir.join("tasks").display()))?;
    if loaded.config.backends().contains(&rbit_workflow::protocol::Backend::Container) {
        backend::container::janitor(&loaded.config).await;
    }
    let shutdown = shutdown::Shutdown::listen(std::time::Duration::from_secs(loaded.config.shutdown_grace_seconds));
    let executor = JobExecutor::new(Arc::new(loaded.clone())).with_server(client.clone());
    poller::Poller::new(client, loaded, Arc::new(executor), once).cancel_tasks_with(shutdown.cancel_tasks).run(shutdown.stop_polling).await
}

/// Logs to standard error; `RUST_LOG` overrides the level.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_ansi(ansi).with_writer(std::io::stderr).compact().init();
}
