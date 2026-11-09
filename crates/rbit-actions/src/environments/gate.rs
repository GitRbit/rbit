//! Whether a run's ref may deploy to an environment, asking for branch protection only
//! when the policy needs it.
use std::future::Future;

use super::{RefPolicy, selected};

/// The repository's branch protection rules, read when asked.
pub trait ProtectedBranches: Sync {
    type Error: Send;

    /// Whether a protection rule matches branch `branch`.
    fn is_protected(&self, branch: &str) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Checks a claimed job's ref against its environment's branch policy.
#[derive(Debug)]
pub struct DeploymentGate;

impl DeploymentGate {
    /// `Ok(Err(reason))` when `policy` refuses `reference` (a full ref) for environment
    /// `environment`; protection is read only for a protected policy and a branch.
    pub async fn allows<P: ProtectedBranches>(
        rules: &P,
        environment: &str,
        policy: RefPolicy<'_>,
        reference: &str,
    ) -> Result<Result<(), String>, P::Error> {
        let (kind, short) = match reference.strip_prefix("refs/heads/") {
            Some(branch) => ("branch", branch),
            None => ("tag", reference.strip_prefix("refs/tags/").unwrap_or(reference)),
        };
        let allowed = match policy {
            RefPolicy::All => true,
            RefPolicy::Protected => kind == "branch" && rules.is_protected(short).await?,
            RefPolicy::Selected(patterns) => selected(patterns, reference),
        };
        Ok(if allowed {
            Ok(())
        } else {
            Err(format!("{kind} `{short}` is not allowed to deploy to {environment} due to environment protection rules"))
        })
    }
}
