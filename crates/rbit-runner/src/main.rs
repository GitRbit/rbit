//! `rbit-runner` — runs rbit Actions jobs (§12.5 of `docs/design/actions.md`).
//!
//! ```text
//! rbit-runner register --url <rbit> --token <registration token> [--name] [--labels]
//! rbit-runner run [--once]          # poll for tasks and run them
//! rbit-runner exec <workflow.yml>   # run a workflow from a local checkout, no server
//! rbit-runner check                 # docker/podman, git and shells on this host
//! rbit-runner install [--mode service|desktop] [--user] | uninstall | start | stop | status
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
mod matchers;
mod node;
mod poller;
mod process;
mod register;
mod service;
mod shutdown;
mod steps;
mod task;
#[cfg(test)]
mod testing;
mod untrusted;
#[cfg(windows)]
mod winservice;

use std::{io::IsTerminal as _, path::Path, sync::Arc};

use anyhow::Context as _;
use clap::Parser as _;
use tracing_subscriber::EnvFilter;

use crate::{
    cli::{Cli, Command},
    client::Client,
    job::JobExecutor,
};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    client::install_crypto_provider();
    if let Command::Run { as_service: Some(name), .. } = &cli.command {
        // The service manager gives a service no console: it logs next to its configuration.
        init_logging(Some(&cli.config.with_file_name("runner.log")));
        #[cfg(windows)]
        return winservice::run(cli.config.clone(), name);
        #[cfg(not(windows))]
        anyhow::bail!("--as-service {name} is for the Windows service manager; `rbit-runner install` sets it up");
    }
    init_logging(None);
    let code = match cli.command {
        Command::Install { service: target, user, no_start } => {
            service::install(&service::plan(&cli.config, target.mode, user, target.name)?, !no_start).map(|()| 0)
        }
        Command::Uninstall(target) => service::uninstall(&service::plan(&cli.config, target.mode, None, target.name)?).map(|()| 0),
        Command::Start(target) => service::start(&service::plan(&cli.config, target.mode, None, target.name)?).map(|()| 0),
        Command::Stop(target) => service::stop(&service::plan(&cli.config, target.mode, None, target.name)?).map(|()| 0),
        Command::Status(target) => {
            service::status(&service::plan(&cli.config, target.mode, None, target.name)?).map(|running| i32::from(!running))
        }
        command => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(async move {
                match command {
                    Command::Register(args) => register::run(&cli.config, args).await,
                    Command::Check => check::run(&cli.config).await,
                    Command::Run { once, .. } => run(&cli.config, once, false).await,
                    Command::Exec(args) => exec::run(&cli.config, args).await,
                    _ => unreachable!("handled above"),
                }
            })
            .map(|()| 0),
    }?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// `rbit-runner run`: polls until Ctrl-C, SIGTERM, a stop request in the work directory or
/// the Windows service manager's stop, then lets running tasks finish.
async fn run(config_path: &Path, once: bool, as_service: bool) -> anyhow::Result<()> {
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
    let grace = std::time::Duration::from_secs(loaded.config.shutdown_grace_seconds);
    let shutdown = shutdown::Shutdown::listen(grace, &loaded.work_dir, !as_service);
    let executor = JobExecutor::new(Arc::new(loaded.clone())).with_server(client.clone());
    poller::Poller::new(client, loaded, Arc::new(executor), once).cancel_tasks_with(shutdown.cancel_tasks).run(shutdown.stop_polling).await
}

/// Logs to standard error, or appends to `file`; `RUST_LOG` overrides the level.
fn init_logging(file: Option<&Path>) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).compact();
    if let Some(file) = file.and_then(|path| std::fs::OpenOptions::new().create(true).append(true).open(path).ok()) {
        builder.with_ansi(false).with_writer(std::sync::Mutex::new(file)).init();
    } else {
        let ansi = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
        builder.with_ansi(ansi).with_writer(std::io::stderr).init();
    }
}
