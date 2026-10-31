//! `rbit` — a self-hosted Git platform.
//!
//! ```text
//! rbit [--config rbit.toml] serve          # run the web server (default)
//! rbit migrate                               # apply database migrations and exit
//! rbit backup --output DIR                  # write a consistent backup to a new directory
//! rbit user create <name> <email> [--admin]  # create an account (password from stdin)
//!
//! rbit auth login --hostname https://git.example.com --with-token < token
//! rbit run list | view [--log-failed] | watch | rerun | cancel | download
//! rbit workflow list | run <file> [--ref r] [-f key=value]
//! rbit artifact list | download <id or name>
//! rbit pr create | list | view | merge | checks
//! rbit api <path> [-X method] [-f key=value]
//! ```
//!
//! The client commands talk to an instance's REST API (see [`client`]); the others run the
//! server on this machine.

mod backup;
mod cli;
mod client;
mod config;
mod serve;
mod ssh;
mod telemetry;

use anyhow::Context;
use clap::Parser;

use crate::cli::{Cli, Command, UserCommand};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Before anything can open a TLS connection (webhook deliveries, mail, mirror checks):
    // every rustls user in the process then agrees on `ring`, whatever its features.
    rbit_services::webhooks::delivery::install_crypto_provider();
    // The client needs no server configuration, and logs nothing of its own.
    if let Some(Command::Client(command)) = cli.command {
        let code = client::run(command).await.unwrap_or_else(|error| {
            eprintln!("rbit: {error:#}");
            1
        });
        std::process::exit(code);
    }
    telemetry::init();
    // `RUSTIT_CONFIG` is the variable's name from before the rename to rbit.
    let file = cli.config.or_else(|| std::env::var_os("RUSTIT_CONFIG").map(std::path::PathBuf::from));
    let config = config::load(file.as_deref()).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => match serve::run(config).await? {
            serve::Stopped::Shutdown => Ok(()),
            serve::Stopped::Restart => restart(),
        },
        Command::Migrate => serve::migrate(config).await,
        Command::Backup { output } => backup::run(config, output).await,
        Command::User { command: UserCommand::Create { username, email, admin } } => {
            serve::create_user(config, username, email, admin).await
        }
        Command::Client(_) => unreachable!("handled before the configuration is loaded"),
    }
}

/// Starts this executable again with the same arguments and environment, after `serve` has
/// shut down cleanly, so the configuration is read afresh. On Unix the new program replaces
/// this process and keeps its id: tini, systemd and container runtimes see no exit, whatever
/// their restart policy.
fn restart() -> anyhow::Result<()> {
    let executable = std::env::current_exe().context("cannot find the rbit executable to restart")?;
    tracing::info!(executable = %executable.display(), "restarting");
    let mut command = std::process::Command::new(executable);
    command.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // Only returns on failure.
        Err(command.exec()).context("cannot restart rbit")
    }
    #[cfg(not(unix))]
    {
        command.spawn().context("cannot restart rbit")?;
        Ok(())
    }
}
