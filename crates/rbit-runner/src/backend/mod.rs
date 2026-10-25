//! Where a job's steps run (§12.5.2, §12.5.3).
//!
//! Each label maps to one backend. A step is described once, as a [`StepCommand`] with
//! paths as the step sees them ([`View`]); the backend turns it into the command that
//! runs it. Built-in actions (checkout, cache) run on the runner host and write into the
//! task's workspace directly, whatever the backend.

pub mod container;
pub mod host;

use std::{collections::BTreeMap, path::PathBuf};

use tokio::process::Command;

/// A step's process, independent of where it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepCommand {
    /// The program: a resolved path on the host, a name inside a container.
    pub program: String,
    pub args: Vec<String>,
    /// Appended to the command line exactly as written, after `args` (Windows `cmd`).
    pub raw_args: Option<String>,
    /// The step's own environment, on top of the backend's.
    pub env: BTreeMap<String, String>,
    /// The working directory, as the step sees it.
    pub cwd: String,
}

/// A task's directories as its steps see them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub workspace: String,
    pub temp: String,
    pub files: String,
}

/// A task's directories on the runner host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub workspace: PathBuf,
    pub temp: PathBuf,
    pub files: PathBuf,
}

impl Dirs {
    /// `<task dir>/{workspace, temp, files}`, created.
    pub fn create(task_dir: &std::path::Path) -> std::io::Result<Self> {
        let dirs = Self { workspace: task_dir.join("workspace"), temp: task_dir.join("temp"), files: task_dir.join("files") };
        for dir in [&dirs.workspace, &dirs.temp, &dirs.files] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(dirs)
    }
}

/// A step's command, with what must live as long as it runs (its env file).
#[derive(Debug)]
pub struct Prepared {
    pub command: Command,
    pub env_file: Option<container::EnvFile>,
}

/// The backend of one job.
#[derive(Debug)]
pub enum Backend {
    Host(host::Host),
    Container(Box<container::Container>),
}

impl Backend {
    pub fn view(&self) -> &View {
        match self {
            Self::Host(host) => host.view(),
            Self::Container(container) => container.view(),
        }
    }

    /// `RUNNER_OS` and `runner.os` as GitHub spells them.
    pub fn os(&self) -> &'static str {
        match self {
            Self::Host(_) => runner_os(std::env::consts::OS),
            Self::Container(_) => "Linux",
        }
    }

    /// The command that runs `step`.
    pub fn command(&self, step: &StepCommand) -> std::io::Result<Prepared> {
        match self {
            Self::Host(host) => Ok(Prepared { command: host.command(step), env_file: None }),
            Self::Container(container) => container.command(step).map(|(command, env_file)| Prepared { command, env_file: Some(env_file) }),
        }
    }

    /// How a running step is stopped.
    pub fn stopper(&self) -> crate::process::Stopper {
        match self {
            Self::Host(_) => crate::process::Stopper::Host,
            Self::Container(container) => container.stopper(),
        }
    }

    /// The program `name` as the step would find it on `path` (the step's `PATH`).
    pub fn find(&self, name: &str, path: &str) -> Option<String> {
        match self {
            Self::Host(_) => crate::process::find_on(name, path).map(|found| found.to_string_lossy().into_owned()),
            Self::Container(container) => container.find(name),
        }
    }

    /// The `PATH` steps start with.
    pub fn base_path(&self) -> String {
        match self {
            Self::Host(host) => host.base_path(),
            Self::Container(container) => container.base_path(),
        }
    }

    /// The separator of `PATH` lists.
    pub fn path_separator(&self) -> char {
        if self.windows() { ';' } else { ':' }
    }

    /// Whether steps run on Windows.
    pub fn windows(&self) -> bool {
        match self {
            Self::Host(_) => cfg!(windows),
            Self::Container(_) => false,
        }
    }
}

/// `linux` → `Linux`, as GitHub's `RUNNER_OS`.
pub fn runner_os(os: &str) -> &'static str {
    match os {
        "windows" => "Windows",
        "macos" => "macOS",
        _ => "Linux",
    }
}

/// `x86_64` → `X64`, as GitHub's `RUNNER_ARCH`.
pub fn runner_arch(arch: &str) -> &'static str {
    match arch {
        "x86" => "X86",
        "aarch64" => "ARM64",
        "arm" => "ARM",
        _ => "X64",
    }
}
