//! Secret pattern construction and the server masking boundary.
use crate::{ActionsService, ServiceError};
use rbit_core::actions::{Job, Task, TaskId};
use rbit_workflow::spec::JobSpec;
use std::sync::{Arc, PoisonError};

// The masking rules are `rbit_actions::logs`; this adapter keeps each task's masker.
pub use rbit_actions::logs::{MASK, MIN_MASKED_CHARS, Masker};

impl ActionsService {
    /// Remembers what `spec`'s task must never store: its secrets and its token.
    pub(in crate::actions) fn remember_masks(&self, spec: &JobSpec) {
        let values = spec.secrets.values().map(rbit_workflow::secret::Secret::expose).chain([spec.token.expose()]);
        let masker = Arc::new(Masker::new(values));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(TaskId(spec.task_id.0), masker);
    }

    /// Forgets a finished task's masks.
    pub(in crate::actions) fn forget_masks(&self, task: TaskId) {
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).remove(&task);
    }

    /// The masker of `task` of `job`: the one built when it was handed out, or else one
    /// rebuilt from its run's secrets.
    pub(in crate::actions) async fn masker(&self, task: &Task, job: &Job) -> Result<Arc<Masker>, ServiceError> {
        if let Some(masker) = self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).get(&task.id) {
            return Ok(Arc::clone(masker));
        }
        let Some(run) = self.ctx().database.actions_runs().run(job.run_id).await? else { return Ok(Arc::new(Masker::none())) };
        let grants = rbit_actions::trust::Grants::of_stored(run.trusted);
        let mut secrets = if grants.secrets_and_variables { self.task_secrets(&run).await? } else { std::collections::BTreeMap::default() };
        // Actions: environments. The job had its environment's secrets too.
        if let Some(environment) = &job.environment
            && let Ok(values) = self.environment_values(&run, environment).await?
        {
            secrets.extend(values.secrets);
        }
        let masker = Arc::new(Masker::new(secrets.values().map(String::as_str)));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(task.id, Arc::clone(&masker));
        Ok(masker)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_tokens_are_masked_by_the_prefix_they_are_minted_with() {
        assert_eq!(rbit_actions::logs::TASK_TOKEN_PREFIX, rbit_core::actions::TASK_TOKEN_PREFIX);
    }
}
