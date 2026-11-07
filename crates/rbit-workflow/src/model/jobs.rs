//! Typed workflow jobs records; expression text remains unevaluated.
use super::calls::JobCall;
use crate::error::Location;
use crate::spec::{Env, RegistryCredentials, RunDefaults, StepAction};
use std::collections::BTreeMap;

/// One coarse permission of the workflow token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Permission {
    #[default]
    None,
    Read,
    Write,
}

/// `permissions:`, coarsely: `contents`, `issues` and `pull-requests`. Every other
/// permission is accepted and ignored. A mapping leaves unnamed permissions at `none`, as
/// on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Permissions {
    pub contents: Permission,
    pub issues: Permission,
    pub pull_requests: Permission,
}

impl Permissions {
    pub const fn uniform(level: Permission) -> Self {
        Self { contents: level, issues: level, pull_requests: level }
    }
}

/// Workflow-level or job-level `concurrency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concurrency {
    /// The group, as written (it may contain expressions).
    pub group: String,
    /// `cancel-in-progress`: `true`, `false` or one expression, as text.
    pub cancel_in_progress: String,
    pub at: Location,
}

/// One job of a workflow.
#[derive(Debug, Clone)]
pub struct Job {
    /// The key under `jobs`: `[A-Za-z_][A-Za-z0-9_-]*`.
    pub id: String,
    /// `name`, as written (it may contain expressions).
    pub name: Option<String>,
    /// The jobs this one waits for; each exists in the workflow, none twice.
    pub needs: Vec<String>,
    /// `if`, as written; `None` means `success()`.
    pub condition: Option<String>,
    /// `runs-on` labels, as written (they may contain expressions).
    pub runs_on: Vec<String>,
    /// Job-level `permissions`; they replace the workflow's.
    pub permissions: Option<Permissions>,
    /// `env`, as templates.
    pub env: Env,
    /// `defaults.run`, as written; the workflow's apply where this sets nothing.
    pub defaults: RunDefaults,
    /// `outputs`, as templates the runner evaluates after the last step.
    pub outputs: BTreeMap<String, String>,
    /// `timeout-minutes` as written (a number or an expression); `None` means
    /// [`crate::spec::DEFAULT_TIMEOUT_MINUTES`].
    pub timeout_minutes: Option<String>,
    /// `continue-on-error` as written (`true`, `false` or an expression).
    pub continue_on_error: Option<String>,
    /// Job-level `concurrency`.
    pub concurrency: Option<Concurrency>,
    pub strategy: Strategy,
    pub container: Option<Container>,
    /// `services`, in file order.
    pub services: Vec<Service>,
    /// At least one, at most [`crate::spec::MAX_STEPS`].
    pub steps: Vec<Step>,
    // Actions: environments.
    /// `environment`: the deployment environment the job runs in.
    pub environment: Option<EnvironmentRef>,
    // Actions: reusable workflows.
    /// `uses`: the job calls a reusable workflow; it then has no `runs-on` and no steps.
    pub call: Option<JobCall>,
    pub at: Location,
}

// Actions: environments.
/// A job's `environment`: `name` (or `{name, url}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRef {
    /// A template the server evaluates when it plans the job (`github`, `inputs`, `vars`,
    /// `matrix`, `strategy`).
    pub name: String,
    /// A template the runner evaluates after the job's steps (it may use `steps`).
    pub url: Option<String>,
}

/// A job's `strategy`.
#[derive(Debug, Clone, Default)]
pub struct Strategy {
    pub matrix: Option<Matrix>,
    /// `fail-fast` as written (`true`, `false` or an expression); `None` means `true`.
    pub fail_fast: Option<String>,
    /// `max-parallel` as written (a number or an expression); `None` means no limit.
    pub max_parallel: Option<String>,
}

/// The job `container`: `container: image` or the mapping form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// The image, as written (it may contain expressions).
    pub image: String,
    pub env: Env,
    /// `credentials`, as templates.
    pub credentials: Option<RegistryCredentials>,
    /// `options`, as written (it may contain expressions; see [`crate::options`]).
    pub options: Option<String>,
    /// `volumes`, as written: named volumes only.
    pub volumes: Vec<String>,
    pub at: Location,
}

/// One `services.<id>` container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The key under `services`: also its host name on the job's network.
    pub id: String,
    /// The image, as written (it may contain expressions).
    pub image: String,
    pub env: Env,
    pub credentials: Option<RegistryCredentials>,
    /// `options`, as written.
    pub options: Option<String>,
    /// `volumes`, as written.
    pub volumes: Vec<String>,
    pub at: Location,
}

/// One step of a job. Everything but `id` may contain expressions for the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// `id`, unique within the job: `[A-Za-z_][A-Za-z0-9_-]*`.
    pub id: Option<String>,
    pub name: Option<String>,
    /// `if`, as written; `None` means `success()`.
    pub condition: Option<String>,
    /// `run`, or `uses` resolved to a built-in ([`crate::builtins`]).
    pub action: StepAction,
    /// `shell` (`run` steps only): one of [`Shell`]'s names, or an expression.
    pub shell: Option<String>,
    /// `working-directory` (`run` steps only).
    pub working_directory: Option<String>,
    pub env: Env,
    /// `continue-on-error` as written (`true`, `false` or an expression).
    pub continue_on_error: Option<String>,
    /// `timeout-minutes` as written (a number or an expression).
    pub timeout_minutes: Option<String>,
    pub at: Location,
}

/// The shells a `run` step may name (§12.2). A custom command template with `{0}` for the
/// script's path (`perl {0}`) is a shell too; the runner runs it as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    Bash,
    Sh,
    Pwsh,
    Powershell,
    Cmd,
    Python,
}

impl Shell {
    pub const ALL: [Self; 6] = [Self::Bash, Self::Sh, Self::Pwsh, Self::Powershell, Self::Cmd, Self::Python];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shell| shell.name() == name)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Sh => "sh",
            Self::Pwsh => "pwsh",
            Self::Powershell => "powershell",
            Self::Cmd => "cmd",
            Self::Python => "python",
        }
    }

    /// The command line that runs a script, `{0}` standing for the script's path. These
    /// are GitHub's, so scripts behave as they do there.
    pub const fn command(self) -> &'static str {
        match self {
            Self::Bash => "bash --noprofile --norc -eo pipefail {0}",
            Self::Sh => "sh -e {0}",
            Self::Pwsh => "pwsh -command \". '{0}'\"",
            Self::Powershell => "powershell -command \". '{0}'\"",
            Self::Cmd => "cmd /D /E:ON /V:OFF /S /C \"CALL \"{0}\"\"",
            Self::Python => "python {0}",
        }
    }

    /// The extension the script file needs for the shell to run it.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Bash | Self::Sh => ".sh",
            Self::Pwsh | Self::Powershell => ".ps1",
            Self::Cmd => ".cmd",
            Self::Python => ".py",
        }
    }
}

/// `strategy.matrix`, as data.
///
/// Strings may hold `${{ }}` over `github`, `inputs` and `vars`: the whole matrix
/// (`${{ fromJSON(inputs.matrix) }}`), a dimension's list, `include`, `exclude` or single
/// values. Planning evaluates them, then checks and expands the result
/// ([`crate::plan::Axes`]). A matrix without expressions was checked when it was parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub value: serde_json::Value,
    /// Whether any string in [`Self::value`] holds `${{ }}`.
    pub has_expressions: bool,
    pub at: Location,
}
