//! What a job is and what a runner is told (§12.3 and §12.8 of `docs/design/actions.md`).
//!
//! * A [`JobTemplate`] is one planned job (one matrix combination), stored with the job when
//!   its run is created. The fields the server evaluates at creation (§12.4, first row) hold
//!   their results: `name`, `runs-on`, the strategy, job `concurrency`, `timeout-minutes`,
//!   job `continue-on-error`, `container.image` and `services.*.image`. Every other
//!   expression stays a template string, to be evaluated by the server when the job
//!   unblocks (job `if`) or by the runner at step time.
//! * A [`JobSpec`] is the template plus what one task needs: the contexts, secrets and
//!   variables, the task token, where to check out from and which cache scopes to use. The
//!   server assembles it on a claim and sends it once, in a [`crate::protocol::PollResponse`].
//!
//! Both are instructions, so both deny unknown fields and carry a version
//! ([`JOB_TEMPLATE_VERSION`], [`JOB_SPEC_VERSION`]): a reader that ignored a field would run
//! a different job (see [`crate::protocol`] for the rule). Secrets and the token are
//! [`crate::secret::Secret`]s, so printing a spec never prints them. Types only: building templates from a
//! plan and evaluating them belong to [`crate::plan`] and [`crate::expr`].

mod cache_scopes;
mod job;
mod limits;
mod permissions;
mod policy;
mod runtime;
mod templates;
mod validation;

pub use self::cache_scopes::{RunCacheScope, cache_ref_scope};
pub use self::job::{CacheScopes, CheckoutSource, Contexts, JobResult, JobSpec, Needed};
pub use self::limits::{
    DEFAULT_TIMEOUT_MINUTES, Env, JOB_SPEC_VERSION, JOB_TEMPLATE_VERSION, MAX_ENVIRONMENT_NAME_BYTES, MAX_NAME_BYTES, MAX_SECRET_BYTES,
    MAX_SECRETS, MAX_SERVICES, MAX_STEPS, RESERVED_NAME_PREFIXES,
};
pub use self::permissions::{Access, TokenPermissions};
pub use self::policy::{ActionsPolicy, ActionsRule};
pub use self::runtime::{
    ActionSource, ActionStep, Builtin, BuiltinKind, ConcurrencyGroup, Container, RegistryCredentials, RunDefaults, Service, StepAction,
    StepTemplate,
};
pub use self::templates::{CallLevel, DeferredInput, JobTemplate, NeedSource, PassedSecrets, Strategy};
pub use self::validation::{valid_environment_name, valid_job_key};

#[cfg(test)]
mod policy_tests;

#[cfg(test)]
mod tests;
