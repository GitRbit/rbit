//! The server's masking pass (§11.10): whatever a runner sends back — job outputs, log
//! lines — is cleaned of the secrets and the token its task received before it is stored.
//!
//! A [`Masker`] is built per task when its job specification is assembled, from every
//! secret value, every line of at least four characters of a multi-line one, and the task
//! token, and is kept in memory while the task runs. After a restart it is rebuilt from the
//! task's secrets alone: the token was never stored. The runner masks first (with base64
//! forms too); this pass is the server's defence in depth.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::sync::{Arc, PoisonError};

use aho_corasick::{AhoCorasick, MatchKind};
use rbit_core::actions::{Job, Task, TaskId};
use rbit_workflow::spec::JobSpec;

use super::ActionsService;
use crate::ServiceError;

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
    pub(super) fn remember_masks(&self, spec: &JobSpec) {
        let values = spec.secrets.values().map(rbit_workflow::secret::Secret::expose).chain([spec.token.expose()]);
        let masker = Arc::new(Masker::new(values));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(TaskId(spec.task_id.0), masker);
    }

    /// Forgets a finished task's masks.
    pub(super) fn forget_masks(&self, task: TaskId) {
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).remove(&task);
    }

    /// The masker of `task` of `job`: the one built when it was handed out, or else one
    /// rebuilt from its run's secrets.
    pub(super) async fn masker(&self, task: &Task, job: &Job) -> Result<Arc<Masker>, ServiceError> {
        if let Some(masker) = self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).get(&task.id) {
            return Ok(Arc::clone(masker));
        }
        let Some(run) = self.ctx().database.actions_runs().run(job.run_id).await? else { return Ok(Arc::new(Masker::none())) };
        let secrets = if run.trusted { self.task_secrets(&run).await? } else { std::collections::BTreeMap::default() };
        let masker = Arc::new(Masker::new(secrets.values().map(String::as_str)));
        self.inner.masks.lock().unwrap_or_else(PoisonError::into_inner).insert(task.id, Arc::clone(&masker));
        Ok(masker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_their_lines_are_masked() {
        let masker = Masker::new(["hunter2-secret", "line-one\nline-two\nab", "abc"]);
        assert_eq!(masker.mask("password=hunter2-secret!"), "password=***!");
        assert_eq!(masker.mask("got line-two and ab"), "got *** and ab", "short lines stay");
        assert_eq!(masker.mask("abc is too short to mask"), "abc is too short to mask");
        assert_eq!(masker.mask("line-one\nline-two\nab"), "***", "the whole value at once");
        assert_eq!(masker.mask("line-one\nline-two"), "***\n***");
        assert_eq!(Masker::none().mask("hunter2-secret"), "hunter2-secret");
        assert_eq!(Masker::new(["x"]).mask("x"), "x");
    }

    #[test]
    fn overlapping_secrets_mask_the_longest() {
        let masker = Masker::new(["token", "token-extended"]);
        assert_eq!(masker.mask("a token-extended b token"), "a *** b ***");
    }
}
