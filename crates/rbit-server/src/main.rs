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

use clap::Parser;

use crate::cli::{Cli, Command, UserCommand};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    #[cfg(windows)]
    let restart_worker = cli.restart_worker;
    // Resolve before serving so replacing the installed executable still restarts
    // the original path, even on systems that label a running old inode "deleted".
    let mut replacement = std::process::Command::new(std::env::current_exe()?);
    replacement.args(std::env::args_os().skip(1));
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(run(cli));
    // Release listeners, workers and redb's exclusive file lock before relaunching.
    drop(runtime);
    if result? {
        #[cfg(windows)]
        if restart_worker {
            // The original process supervises every replacement, so repeated
            // restarts keep a fixed two-process tree and a stable service PID.
            std::process::exit(75);
        }
        relaunch(&mut replacement)?;
    }
    Ok(())
}

fn relaunch(command: &mut std::process::Command) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // Replaces this process, preserving the PID tracked by systemd and Docker.
        Err(command.exec())
    }
    #[cfg(windows)]
    {
        use std::io::IsTerminal as _;
        use std::os::windows::process::CommandExt as _;
        command.arg("--restart-worker");
        if !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal() && !std::io::stderr().is_terminal() {
            // Preserve an existing interactive console, and keep headless servers hidden.
            command.creation_flags(0x0800_0000);
        }
        loop {
            let mut child = command.spawn()?;
            tracing::info!(process_id = child.id(), "replacement application started");
            let status = child.wait()?;
            if status.code() == Some(75) {
                continue;
            }
            return if status.success() { Ok(()) } else { Err(std::io::Error::other(format!("replacement application exited: {status}"))) };
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = command;
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "application restarts are unsupported on this platform"))
    }
}

async fn run(cli: Cli) -> anyhow::Result<bool> {
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
        Command::Serve => serve::run(config).await,
        Command::Migrate => serve::migrate(config).await.map(|()| false),
        Command::Backup { output } => backup::run(config, output).await.map(|()| false),
        Command::User { command: UserCommand::Create { username, email, admin } } => {
            serve::create_user(config, username, email, admin).await.map(|()| false)
        }
        Command::Client(_) => unreachable!("handled before the configuration is loaded"),
    }
}
