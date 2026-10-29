//! Command-line interface (§12.5.1 of `docs/design/actions.md`).

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "rbit-runner", version, about = "Runs rbit Actions jobs in containers or a host shell")]
pub struct Cli {
    /// The runner's configuration file. The runner credential is kept next to it, in
    /// `.credential`.
    #[arg(short, long, env = "RBIT_RUNNER_CONFIG", default_value = "runner.toml", global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Exchange a registration token for a runner credential, and write `runner.toml` and
    /// `.credential`.
    Register(RegisterArgs),
    /// Poll rbit for jobs and run them.
    Run {
        /// Run one task, then unregister and exit (an ephemeral runner).
        #[arg(long)]
        once: bool,
        /// Run as the Windows Service of this name; the service manager starts it so.
        #[arg(long, hide = true, value_name = "NAME")]
        as_service: Option<String>,
    },
    /// Install the runner with the system's service manager (systemd, launchd, Windows),
    /// to start at boot or at logon and restart when it fails; starts it unless --no-start.
    Install {
        #[command(flatten)]
        service: ServiceArgs,
        /// The account a `service` runs as: a Linux or macOS user, or a Windows account
        /// (`NT AUTHORITY\NETWORK SERVICE` by default; a password in
        /// `RBIT_RUNNER_SERVICE_PASSWORD`).
        #[arg(long)]
        user: Option<String>,
        /// Install without starting it.
        #[arg(long)]
        no_start: bool,
    },
    /// Stop the installed runner gracefully and remove it from the service manager.
    Uninstall(ServiceArgs),
    /// Start the installed runner.
    Start(ServiceArgs),
    /// Stop the installed runner gracefully: running tasks get `shutdown_grace_seconds`.
    Stop(ServiceArgs),
    /// Whether the runner is installed and running; exits with 1 unless it runs.
    Status(ServiceArgs),
    /// Plan and run a workflow from a local checkout, without a server.
    Exec(ExecArgs),
    /// Check the configured backends: docker or podman, git, and the shells.
    Check,
}

#[derive(Debug, clap::Args)]
pub struct RegisterArgs {
    /// The rbit instance, e.g. `https://git.example.com`.
    #[arg(long)]
    pub url: String,
    /// The registration token from the runners page. Prefer the environment variable,
    /// which other users of this host cannot read from the process list.
    #[arg(long, env = "RBIT_RUNNER_TOKEN", hide_env_values = true)]
    pub token: String,
    /// The runner's name; defaults to the host name.
    #[arg(long)]
    pub name: Option<String>,
    /// `runs-on` labels as `name:backend[:image]`, comma-separated or repeated, e.g.
    /// `ubuntu-latest:container:docker.io/library/ubuntu:24.04,self-hosted:host`. A
    /// container label needs an image. Defaults to `ubuntu-latest` in an Ubuntu container.
    #[arg(long, value_delimiter = ',')]
    pub labels: Vec<String>,
    /// Tasks to run at once.
    #[arg(long, default_value_t = 1)]
    pub capacity: u32,
    /// Where task workspaces and the cache live; relative to the configuration file.
    #[arg(long, default_value = "work")]
    pub work_dir: PathBuf,
    /// The container CLI: `docker` or `podman`.
    #[arg(long, default_value = "docker")]
    pub container_program: String,
    /// Allow a plain `http://` URL, for a server on this machine only.
    #[arg(long)]
    pub insecure_http: bool,
    /// Replace an existing registration in this directory.
    #[arg(long)]
    pub replace: bool,
}

/// Which installed runner a service command means.
#[derive(Debug, clap::Args)]
pub struct ServiceArgs {
    /// `service`: a system service, started at boot without a desktop. `desktop`: in the
    /// user's session, started at logon, for jobs that need the desktop.
    #[arg(long, value_enum, default_value_t = crate::service::Mode::Service)]
    pub mode: crate::service::Mode,
    /// The service's name; `rbit-runner.<runner name>` by default.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct ExecArgs {
    /// The workflow file, inside a local checkout.
    pub workflow: PathBuf,
    /// Run only this job (and nothing it needs).
    #[arg(long)]
    pub job: Option<String>,
    /// The event to plan the workflow for.
    #[arg(long, default_value = "push")]
    pub event: String,
    /// Where the jobs run.
    #[arg(long, value_enum, default_value_t = ExecBackend::Container)]
    pub backend: ExecBackend,
    /// A secret to pass to the jobs, read from the environment variable of the same name.
    #[arg(long = "secret", value_name = "NAME")]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExecBackend {
    Host,
    Container,
}
