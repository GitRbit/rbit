//! The host backend (§12.5.3): steps are child processes of the runner's user, in the
//! task's workspace, with no isolation.
//!
//! Steps inherit the runner's environment, except the `RBIT_RUNNER_*` variables that
//! configure the runner itself, and get the step's environment on top. On Windows,
//! variable names are compared without case, as Windows does, but each is passed with the
//! spelling it was last set with: a step's `toolchain` stays `$toolchain` in Git Bash.
//! Windows steps run PowerShell scripts as on GitHub's machines
//! (`PSExecutionPolicyPreference=Bypass`, unless the runner's environment sets it).

use std::collections::BTreeMap;

use tokio::process::Command;

use super::{Dirs, StepCommand, View};

/// The host backend of one job.
#[derive(Debug)]
pub struct Host {
    view: View,
    /// By [`env_key`]: the name as spelt, and the value.
    base_env: BTreeMap<String, (String, String)>,
}

impl Host {
    pub fn new(dirs: &Dirs) -> Self {
        let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let mut base_env: BTreeMap<String, (String, String)> = std::env::vars()
            .filter(|(name, _)| !name.to_ascii_uppercase().starts_with("RBIT_RUNNER_"))
            .map(|(name, value)| (env_key(&name), (name, value)))
            .collect();
        // Actions: third-party actions. GitHub's Windows machines let PowerShell run scripts,
        // and tool installers (`actions/setup-python`'s `setup.ps1`) count on it; a stock
        // Windows does not. The policy is no security boundary, so steps get it for their own
        // processes unless the runner's environment chooses one.
        if cfg!(windows) {
            let name = "PSExecutionPolicyPreference";
            base_env.entry(env_key(name)).or_insert_with(|| (name.to_owned(), "Bypass".to_owned()));
        }
        Self { view: View { workspace: text(&dirs.workspace), temp: text(&dirs.temp), files: text(&dirs.files) }, base_env }
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn base_path(&self) -> String {
        self.base_env.get(&env_key("PATH")).map(|(_, value)| value.clone()).unwrap_or_default()
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
        env.extend(step.env.iter().map(|(name, value)| (env_key(name), (name.clone(), value.clone()))));
        command.env_clear().envs(env.into_values()).current_dir(&step.cwd);
        crate::process::isolate(&mut command);
        command
    }
}

/// What two names that are one variable have in common: the name, upper-cased on Windows.
pub fn env_key(name: &str) -> String {
    if cfg!(windows) { name.to_ascii_uppercase() } else { name.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn names_keep_their_spelling() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs::create(dir.path()).unwrap();
        let host = Host::new(&dirs);
        let shell = if cfg!(windows) { ("cmd", vec!["/C".to_owned(), "set".to_owned()]) } else { ("env", Vec::new()) };
        let program = crate::process::find_program(shell.0).unwrap().display().to_string();
        let env = BTreeMap::from([("toolchain".to_owned(), "stable".to_owned()), ("Mixed_Case".to_owned(), "1".to_owned())]);
        let step = StepCommand { program, args: shell.1, raw_args: None, env, cwd: host.view().workspace.clone() };
        let output = host.command(&step).output().await.unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.lines().any(|line| line == "toolchain=stable"), "{text}");
        assert!(text.lines().any(|line| line == "Mixed_Case=1"), "{text}");
        if cfg!(windows) {
            assert!(text.lines().any(|line| line.eq_ignore_ascii_case("PSExecutionPolicyPreference=Bypass")), "{text}");
        }
    }
}
