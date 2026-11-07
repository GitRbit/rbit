//! Detect needs-dependent fields and preserve deferred stand-in templates.
use crate::expr::Template;
use crate::model::Job;
use crate::spec::{DEFAULT_TIMEOUT_MINUTES, JOB_TEMPLATE_VERSION, JobTemplate, RunDefaults, Strategy};
use serde_json::Map;
use std::collections::BTreeMap;

// Actions: plans from needs.
/// Whether what the server plans of `job` (`name`, `runs-on`, `strategy`, `timeout-minutes`,
/// `continue-on-error`, `concurrency`, `container`, `services`, `environment`) reads `needs`,
/// as GitHub allows: then it is planned when they are done.
pub(super) fn reads_needs(job: &Job) -> bool {
    if job.needs.is_empty() {
        return false;
    }
    let mut texts: Vec<&str> = job.runs_on.iter().map(String::as_str).collect();
    texts.extend(job.name.as_deref());
    texts.extend(job.timeout_minutes.as_deref());
    texts.extend(job.continue_on_error.as_deref());
    texts.extend(job.concurrency.iter().flat_map(|concurrency| [concurrency.group.as_str(), concurrency.cancel_in_progress.as_str()]));
    let matrix = job.strategy.matrix.as_ref().filter(|matrix| matrix.has_expressions).map(|matrix| matrix.value.to_string());
    texts.extend(matrix.as_deref());
    texts.extend(job.strategy.fail_fast.as_deref());
    texts.extend(job.strategy.max_parallel.as_deref());
    for (image, options, volumes) in job
        .container
        .iter()
        .map(|container| (&container.image, &container.options, &container.volumes))
        .chain(job.services.iter().map(|service| (&service.image, &service.options, &service.volumes)))
    {
        texts.push(image);
        texts.extend(options.as_deref());
        texts.extend(volumes.iter().map(String::as_str));
    }
    if let Some(environment) = &job.environment {
        texts.push(&environment.name);
    }
    texts.iter().any(|text| mentions_needs(text))
}

/// Whether `text` has an expression that reads the `needs` context (names ignore case); a
/// text that does not parse (a matrix's JSON, quotes escaped) is searched instead.
fn mentions_needs(text: &str) -> bool {
    if !text.contains("${{") {
        return false;
    }
    match Template::parse(text) {
        Ok(template) => template.contexts().contains(&"needs"),
        Err(_) => text.to_ascii_lowercase().contains("needs"),
    }
}

/// Whether a job's `strategy` reads `needs`.
pub(super) fn strategy_reads_needs(job: &Job) -> bool {
    let matrix = job.strategy.matrix.as_ref().filter(|matrix| matrix.has_expressions).map(|matrix| matrix.value.to_string());
    matrix
        .iter()
        .map(String::as_str)
        .chain(job.strategy.fail_fast.as_deref())
        .chain(job.strategy.max_parallel.as_deref())
        .any(mentions_needs)
}

/// A job's `if:` as an expression (with or without `${{ }}`), `None` when it does not parse.
fn condition_expr(condition: &str) -> Option<crate::expr::Expr> {
    let text = condition.trim();
    if text.contains("${{") { Template::parse(text).ok()?.as_single_expr().cloned() } else { crate::expr::Expr::parse(text).ok() }
}

/// Whether `condition` calls `success()`, `failure()`, `always()` or `cancelled()`.
fn has_status_function(condition: &str) -> bool {
    condition_expr(condition).is_some_and(|expr| expr.uses_status())
}

/// Whether a job's `if:` reads `matrix` or `strategy`.
pub(super) fn reads_matrix(condition: &str) -> bool {
    condition_expr(condition).is_some_and(|expr| expr.contexts().iter().any(|name| matches!(*name, "matrix" | "strategy")))
}

/// The blocked stand-in of a job planned once its needs are done ([`JobTemplate::deferred`]):
/// its `if:` (GitHub's default `success()` when it has none) is decided first.
pub(super) fn deferred_template(job: &Job, key: &str, needs: &[String]) -> JobTemplate {
    JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: key.to_owned(),
        name: job.name.clone().filter(|name| !name.contains("${{")).unwrap_or_else(|| job.id.clone()),
        needs: needs.to_vec(),
        condition: Some(match &job.condition {
            // Its matrix is not known yet: each planned job decides its `if:` (`plan_deferred`).
            // Without a status function an `if:` means `success() && …`: that gate stays.
            Some(condition) if reads_matrix(condition) && has_status_function(condition) => "always()".to_owned(),
            Some(condition) if reads_matrix(condition) => "success()".to_owned(),
            Some(condition) => condition.clone(),
            None => "success()".to_owned(),
        }),
        runs_on: Vec::new(),
        matrix: Map::new(),
        strategy: Strategy { fail_fast: true, max_parallel: None, job_index: 0, job_total: 1 },
        permissions: None,
        concurrency: None,
        timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        continue_on_error: false,
        workflow_env: BTreeMap::new(),
        env: BTreeMap::new(),
        defaults: RunDefaults::default(),
        outputs: BTreeMap::new(),
        container: None,
        services: Vec::new(),
        steps: Vec::new(),
        environment: None,
        environment_url: None,
        calls: Vec::new(),
        need_names: BTreeMap::new(),
        deferred: true,
        default_ceiling: false,
    }
}
