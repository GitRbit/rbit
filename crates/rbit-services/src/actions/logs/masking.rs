//! Secret pattern construction and the server masking boundary.
use crate::{ActionsService, ServiceError};
use aho_corasick::{AhoCorasick, MatchKind};
use rbit_core::actions::{Job, Task, TaskId};
use rbit_workflow::spec::JobSpec;
use std::sync::{Arc, PoisonError};

/// Masked text becomes this.
pub const MASK: &str = "***";
/// Shorter values are not masked: they would blank out ordinary words.
pub const MIN_MASKED_CHARS: usize = 4;

/// Replaces every known secret in text with [`MASK`].
#[derive(Debug, Clone)]
pub struct Masker {
    automaton: Option<AhoCorasick>,
}

impl Masker {
    /// Masks every value, and every line of a multi-line value, of at least
    /// [`MIN_MASKED_CHARS`] characters.
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let mut patterns: Vec<&str> = Vec::new();
        for value in values {
            patterns.push(value);
            if value.contains('\n') {
                patterns.extend(value.lines().map(|line| line.trim_end_matches('\r')));
            }
        }
        patterns.retain(|pattern| pattern.chars().count() >= MIN_MASKED_CHARS);
        patterns.sort_unstable();
        patterns.dedup();
        let automaton =
            if patterns.is_empty() { None } else { AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(&patterns).ok() };
        Self { automaton }
    }

    /// Masks nothing.
    pub fn none() -> Self {
        Self { automaton: None }
    }

    /// `text` with every secret replaced.
    pub fn mask(&self, text: &str) -> String {
        match &self.automaton {
            Some(automaton) if automaton.is_match(text) => automaton.replace_all(text, &vec![MASK; automaton.patterns_len()]),
            _ => text.to_owned(),
        }
    }
}

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
        let mut secrets = if run.trusted { self.task_secrets(&run).await? } else { std::collections::BTreeMap::default() };
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
