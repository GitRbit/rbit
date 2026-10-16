//! `rustit` — a self-hosted Git platform.
//!
//! ```text
//! rustit [--config rustit.toml] serve          # run the web server (default)
//! rustit migrate                               # apply database migrations and exit
//! rustit user create <name> <email> [--admin]  # create an account (password from stdin)
//! ```

mod backup;
mod cli;
mod config;
mod serve;
mod ssh;
mod telemetry;

use clap::Parser;

use crate::cli::{Cli, Command, UserCommand};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    telemetry::init();
    let config = config::load(cli.config.as_deref())?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve::run(config).await,
        Command::Migrate => serve::migrate(config).await,
        Command::Backup { output } => backup::run(config, output).await,
        Command::User { command: UserCommand::Create { username, email, admin } } => {
            serve::create_user(config, username, email, admin).await
        }
    }
}
