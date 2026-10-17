//! Command-line interface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rbit", version, about = "A fast, self-hosted Git platform")]
pub struct Cli {
    /// Path to a TOML configuration file. Settings can also be given as `RBIT_*` environment
    /// variables, e.g. `RBIT_SERVER__BIND=0.0.0.0:3000`.
    #[arg(short, long, env = "RBIT_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the web server (the default).
    Serve,
    /// Apply pending database migrations, then exit.
    Migrate,
    /// Write a consistent backup of all data to a new directory.
    Backup {
        /// Directory to create; it must not exist yet.
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Manage user accounts.
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create an account. The password is read from standard input.
    Create {
        username: String,
        email: String,
        /// Grant site administrator rights.
        #[arg(long)]
        admin: bool,
    },
}
