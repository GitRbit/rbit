//! `rbit` — a self-hosted Git platform.
//!
//! ```text
//! rbit [--config rbit.toml] serve          # run the web server (default)
//! rbit migrate                               # apply database migrations and exit
//! rbit user create <name> <email> [--admin]  # create an account (password from stdin)
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
    // Before anything can open a TLS connection (webhook deliveries, mail, mirror checks):
    // every rustls user in the process then agrees on `ring`, whatever its features.
    rbit_services::webhooks::delivery::install_crypto_provider();
    // `RUSTIT_CONFIG` is the variable's name from before the rename to Rbit.
    let file = cli.config.or_else(|| std::env::var_os("RUSTIT_CONFIG").map(std::path::PathBuf::from));
    let config = config::load(file.as_deref())?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve::run(config).await,
        Command::Migrate => serve::migrate(config).await,
        Command::Backup { output } => backup::run(config, output).await,
        Command::User { command: UserCommand::Create { username, email, admin } } => {
            serve::create_user(config, username, email, admin).await
        }
    }
}
