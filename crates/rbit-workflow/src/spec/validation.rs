//! Spec validation contracts and original validation.
use super::limits::{Env, MAX_ENVIRONMENT_NAME_BYTES, MAX_NAME_BYTES};
use crate::parse::valid_job_id;
use crate::protocol::{ValidationError, ensure};

// Actions: reusable workflows.
/// A job's key in its run: a job id, or for a called workflow's job the calling jobs' ids
/// and its own, `/`-separated, a calling matrix job's with `#<index>` (`call#1/build`); at
/// most [`crate::plan::MAX_JOB_KEY_BYTES`].
pub fn valid_job_key(key: &str) -> bool {
    key.len() <= crate::plan::MAX_JOB_KEY_BYTES
        && key.split('/').all(|part| {
            let id = match part.rsplit_once('#') {
                Some((id, index)) if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()) => id,
                Some(_) => return false,
                None => part,
            };
            valid_job_id(id)
        })
}

// Actions: environments.
/// An environment's name: 1 to [`crate::spec::MAX_ENVIRONMENT_NAME_BYTES`] bytes, no control
/// characters, `/` or `\\`, and not blank at either end. GitHub allows little more.
pub fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ENVIRONMENT_NAME_BYTES
        && name.trim() == name
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
}

/// Environment variable names: 1 to [`crate::spec::MAX_NAME_BYTES`], without `=` or NUL.
pub(super) fn validate_env(env: &Env) -> Result<(), ValidationError> {
    for name in env.keys() {
        ensure(!name.is_empty() && name.len() <= MAX_NAME_BYTES && !name.contains(['=', '\0']), || {
            format!("environment variable name `{name}` is malformed")
        })?;
    }
    Ok(())
}
