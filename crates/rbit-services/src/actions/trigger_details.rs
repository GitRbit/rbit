//! What the run page says about how a run started, beyond the run's row: the cron of a
//! scheduled run and the inputs of a manual one, from the stored event (`event_payload`,
//! `dispatch_inputs`), bounded for a page.
use rbit_core::actions::Run;
use serde_json::{Map, Value};

use crate::{ActionsService, RepoContext, ServiceError};

/// Dispatch inputs listed on a page; the rest are counted.
pub const MAX_LISTED_INPUTS: usize = 20;
/// Longest input value shown, in characters.
pub const MAX_INPUT_CHARS: usize = 200;

/// How a run started, as far as its stored event tells.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriggerDetails {
    /// The cron expression of a scheduled run.
    pub schedule: Option<String>,
    /// A manual run's inputs, by name, as text.
    pub inputs: Vec<(String, String)>,
    /// Inputs not listed.
    pub more_inputs: usize,
}

impl ActionsService {
    /// The trigger details of `run`, a run of `repo` a reader opened.
    pub async fn trigger_details(&self, repo: &RepoContext, run: &Run) -> Result<TriggerDetails, ServiceError> {
        self.require_pages(repo).await?;
        if run.repo_id != repo.repo.repo.id {
            return Err(ServiceError::NotFound);
        }
        let Some((payload, inputs)) = self.ctx().database.actions_runs().event(run.id).await? else {
            return Ok(TriggerDetails::default());
        };
        Ok(details(&payload, inputs.as_deref()))
    }
}

/// The details in a stored event payload and dispatch inputs (JSON).
pub fn details(payload: &str, inputs: Option<&str>) -> TriggerDetails {
    let payload: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let schedule = payload.get("schedule").and_then(Value::as_str).map(str::to_owned);
    let inputs: Map<String, Value> = inputs
        .and_then(|inputs| serde_json::from_str(inputs).ok())
        .or_else(|| payload.get("inputs").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let mut names: Vec<&String> = inputs.keys().collect();
    names.sort();
    let listed: Vec<(String, String)> = names
        .into_iter()
        .take(MAX_LISTED_INPUTS)
        .filter_map(|name| Some((name, inputs.get(name)?)))
        .map(|(name, value)| {
            let text = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            (name.clone(), text.chars().take(MAX_INPUT_CHARS).collect())
        })
        .collect();
    TriggerDetails { schedule, more_inputs: inputs.len() - listed.len(), inputs: listed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_and_inputs_are_read_from_the_event() {
        let scheduled = details(r#"{"schedule":"*/15 * * * *"}"#, None);
        assert_eq!(scheduled.schedule.as_deref(), Some("*/15 * * * *"));
        assert_eq!(scheduled.inputs, Vec::<(String, String)>::new());
        let manual = details(r#"{"inputs":{"a":"x"}}"#, Some(r#"{"debug":true,"level":"info","count":3}"#));
        assert_eq!(manual.inputs, [("count".into(), "3".into()), ("debug".into(), "true".into()), ("level".into(), "info".into())]);
        let fallback = details(r#"{"inputs":{"a":"x"}}"#, None);
        assert_eq!(fallback.inputs, [("a".into(), "x".into())], "the payload's inputs without typed ones");
        assert_eq!(details("not json", Some("[1]")), TriggerDetails::default());
        let many: Map<String, Value> = (0..30).map(|index| (format!("i{index:02}"), Value::String("v".repeat(500)))).collect();
        let bounded = details("{}", Some(&Value::Object(many).to_string()));
        assert_eq!((bounded.inputs.len(), bounded.more_inputs), (MAX_LISTED_INPUTS, 10));
        assert_eq!(bounded.inputs[0].1.chars().count(), MAX_INPUT_CHARS);
    }
}
