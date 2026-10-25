//! Where a job's steps run (§12.5.2, §12.5.3).
//!
//! Each label maps to one backend. A step is described once, as a [`StepCommand`] with
//! paths as the step sees them ([`View`]); the backend turns it into the command that
//! runs it. Built-in actions (checkout, cache) run on the runner host and write into the
//! task's workspace directly, whatever the backend.

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

/// The backend of one job.
#[derive(Debug)]
pub enum Backend {
    Host(host::Host),
}

impl Backend {
    pub fn view(&self) -> &View {
        match self {
            Self::Host(host) => host.view(),
        }
    }

    /// `RUNNER_OS` and `runner.os` as GitHub spells them.
    pub fn os(&self) -> &'static str {
        match self {
            Self::Host(_) => runner_os(std::env::consts::OS),
        }
    }

    /// The command that runs `step`.
    pub fn command(&self, step: &StepCommand) -> Command {
        match self {
            Self::Host(host) => host.command(step),
        }
    }

    /// The program `name` as the step would find it on `path` (the step's `PATH`).
    pub fn find(&self, name: &str, path: &str) -> Option<String> {
        match self {
            Self::Host(_) => crate::process::find_on(name, path).map(|found| found.to_string_lossy().into_owned()),
        }
    }

    /// The `PATH` steps start with.
    pub fn base_path(&self) -> String {
        match self {
            Self::Host(host) => host.base_path(),
        }
    }

    /// The separator of `PATH` lists.
    pub fn path_separator(&self) -> char {
        match self {
            Self::Host(_) => {
                if cfg!(windows) {
                    ';'
                } else {
                    ':'
                }
            }
        }
    }

    /// Whether steps run on Windows.
    pub fn windows(&self) -> bool {
        match self {
            Self::Host(_) => cfg!(windows),
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
