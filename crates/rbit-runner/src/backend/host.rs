//! The host backend (§12.5.3): steps are child processes of the runner's user, in the
//! task's workspace, with no isolation.
//!
//! Steps inherit the runner's environment, except the `RBIT_RUNNER_*` variables that
//! configure the runner itself, and get the step's environment on top. On Windows,
//! variable names are compared without case, as Windows does.

use std::collections::BTreeMap;

use tokio::process::Command;

use super::{Dirs, StepCommand, View};

/// The host backend of one job.
#[derive(Debug)]
pub struct Host {
    view: View,
    base_env: BTreeMap<String, String>,
}

impl Host {
    pub fn new(dirs: &Dirs) -> Self {
        let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let base_env = std::env::vars()
            .filter(|(name, _)| !name.to_ascii_uppercase().starts_with("RBIT_RUNNER_"))
            .map(|(name, value)| (env_key(&name), value))
            .collect();
        Self { view: View { workspace: text(&dirs.workspace), temp: text(&dirs.temp), files: text(&dirs.files) }, base_env }
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn base_path(&self) -> String {
        self.base_env.get(&env_key("PATH")).cloned().unwrap_or_default()
    }

    pub fn command(&self, step: &StepCommand) -> Command {
        let mut command = Command::new(&step.program);
        command.args(&step.args);
        // Only Windows' `cmd` needs its command line as written; elsewhere it is unused.
        #[cfg(windows)]
        if let Some(raw) = &step.raw_args {
            command.raw_arg(raw);
        }
        #[cfg(not(windows))]
        let _ = &step.raw_args;
        let mut env = self.base_env.clone();
        env.extend(step.env.iter().map(|(name, value)| (env_key(name), value.clone())));
        command.env_clear().envs(env).current_dir(&step.cwd);
        crate::process::isolate(&mut command);
        command
    }
}

/// `name` as the environment block stores it: upper-cased on Windows.
pub fn env_key(name: &str) -> String {
    if cfg!(windows) { name.to_ascii_uppercase() } else { name.to_owned() }
}
