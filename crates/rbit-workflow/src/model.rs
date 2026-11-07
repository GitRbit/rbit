//! The typed model of a workflow file: every key of the subset in §12.2 of
//! `docs/design/actions.md`, at every level.
//!
//! A [`Workflow`] only exists once [`crate::parse`] has validated it: no key is unknown,
//! job ids are well formed, `needs` name existing jobs and form no cycle, filter patterns
//! compile, every `${{ }}` parses and uses only the contexts its field is evaluated with,
//! each step has exactly one of `run` and `uses`, and the file is within its
//! [`crate::Limits`].
//!
//! Values that hold expressions are kept as written (template strings); planning
//! evaluates the ones the server decides (§12.4) and the runner evaluates the rest.

mod calls;
mod events;
mod jobs;
mod workflow;

pub use self::calls::{CallInput, CallInputKind, CallSecrets, CallTarget, JobCall, WorkflowCall};
pub use self::events::{
    ActivityType, Dispatch, DispatchInput, Filter, InputKind, PullRequestFilters, PushFilters, RepositoryDispatchFilters, Schedule,
    WorkflowRunFilters, WorkflowRunType,
};
pub use self::jobs::{Concurrency, Container, EnvironmentRef, Job, Matrix, Permission, Permissions, Service, Shell, Step, Strategy};
pub use self::workflow::{On, Workflow};

pub use crate::spec::{ActionSource, ActionStep, Builtin, BuiltinKind, Env, RegistryCredentials, RunDefaults, StepAction};
