//! Whether a run's ref may deploy to an environment, asking for branch protection only
//! when the policy needs it.
use std::future::Future;

use super::{BranchPolicy, full_ref_patterns};

/// The repository's branch protection rules and the selected-pattern matcher (the glob
/// engine of `rbit_workflow::pattern`, which this crate does not link).
pub trait RefRules: Sync {
    type Error: Send;

    /// Whether a protection rule matches branch `branch`; read when asked.
    fn is_protected(&self, branch: &str) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Whether these full-ref patterns ([`full_ref_patterns`]) admit `reference`, a full
    /// ref: the last pattern that matches decides.
    fn selects(&self, patterns: &[String], reference: &str) -> bool;
}

/// Checks a claimed job's ref against its environment's branch policy.
#[derive(Debug)]
pub struct DeploymentGate;

impl DeploymentGate {
    /// `Ok(Err(reason))` when `policy` refuses `reference` (a full ref) for environment
    /// `environment`; protection is read only for a protected policy and a branch.
    pub async fn allows<P: RefRules>(
        rules: &P,
        environment: &str,
        policy: &BranchPolicy,
        reference: &str,
    ) -> Result<Result<(), String>, P::Error> {
        let (kind, short) = match reference.strip_prefix("refs/heads/") {
            Some(branch) => ("branch", branch),
            None => ("tag", reference.strip_prefix("refs/tags/").unwrap_or(reference)),
        };
        let allowed = match policy {
            BranchPolicy::All => true,
            BranchPolicy::Protected => kind == "branch" && rules.is_protected(short).await?,
            BranchPolicy::Selected(patterns) => rules.selects(&full_ref_patterns(patterns), reference),
        };
        Ok(if allowed {
            Ok(())
        } else {
            Err(format!("{kind} `{short}` is not allowed to deploy to {environment} due to environment protection rules"))
        })
    }
}
