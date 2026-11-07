//! Spec limits contracts and original validation.
use std::collections::BTreeMap;

/// The version of the stored [`crate::spec::JobTemplate`] format.
pub const JOB_TEMPLATE_VERSION: u32 = 1;

/// The version of the [`crate::spec::JobSpec`] format sent to runners.
pub const JOB_SPEC_VERSION: u32 = 1;

/// Most steps in one job.
pub const MAX_STEPS: usize = 256;

/// Most service containers in one job.
pub const MAX_SERVICES: usize = 16;

/// `timeout-minutes` of a job that sets none.
pub const DEFAULT_TIMEOUT_MINUTES: u32 = 360;

/// Most secrets one task receives.
pub const MAX_SECRETS: usize = 1024;

/// Largest secret value, in bytes.
pub const MAX_SECRET_BYTES: usize = 64 * 1024;

/// Longest secret, variable or environment variable name, in bytes.
pub const MAX_NAME_BYTES: usize = 100;

/// Prefixes of the names runners set themselves, which no secret or variable may take
/// (`rbit_core::actions::RESERVED_NAME_PREFIXES`). The task token is [`crate::spec::JobSpec::token`],
/// never a `GITHUB_TOKEN` secret.
pub const RESERVED_NAME_PREFIXES: [&str; 4] = ["GITHUB_", "GITEA_", "FORGEJO_", "RBIT_"];

/// Environment variables by name; the values are templates.
pub type Env = BTreeMap<String, String>;

/// Longest environment name.
pub const MAX_ENVIRONMENT_NAME_BYTES: usize = 255;
