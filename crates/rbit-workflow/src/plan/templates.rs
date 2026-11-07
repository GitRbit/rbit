//! Construct unchanged job, runtime and step wire templates.
use super::contexts::JobContexts;
use super::evaluation::Evaluator;
use super::matrix::{Combination, matrix_name};
use crate::error::WorkflowError;
use crate::expr::ContextMap;
use crate::limits::Limits;
use crate::model::{Job, Step, Workflow};
use crate::spec::{
    ConcurrencyGroup, Container, DEFAULT_TIMEOUT_MINUTES, JOB_TEMPLATE_VERSION, JobTemplate, RunDefaults, Service, StepTemplate, Strategy,
};
use serde_json::{Value, json};

/// The `strategy` context of `strategy`.
pub(super) fn strategy_context(strategy: Strategy) -> Value {
    json!({
        "fail-fast": strategy.fail_fast,
        "job-index": strategy.job_index,
        "job-total": strategy.job_total,
        "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
    })
}

/// One job of one combination as a template.
/// The job's `container` and `services`, evaluated.
fn containers(evaluator: &Evaluator<'_>, job: &Job) -> Result<(Option<Container>, Vec<Service>), WorkflowError> {
    let container = match &job.container {
        Some(container) => Some(Container {
            image: evaluator.image("container.image", &container.image)?,
            env: container.env.clone(),
            credentials: container.credentials.clone(),
            options: evaluator.options("container", container.options.as_deref(), false)?,
            volumes: evaluator.volumes("container", &container.volumes)?,
        }),
        None => None,
    };
    let services = job
        .services
        .iter()
        .map(|service| {
            Ok(Service {
                id: service.id.clone(),
                image: evaluator.image(&format!("services.{}.image", service.id), &service.image)?,
                env: service.env.clone(),
                credentials: service.credentials.clone(),
                options: evaluator.options(&format!("services.{}", service.id), service.options.as_deref(), true)?,
                volumes: evaluator.volumes(&format!("services.{}", service.id), &service.volumes)?,
            })
        })
        .collect::<Result<_, WorkflowError>>()?;
    Ok((container, services))
}

pub(super) fn job_template(
    workflow: &Workflow,
    job: &Job,
    run: &ContextMap,
    combination: Combination,
    strategy: Strategy,
    limits: &Limits,
) -> Result<JobTemplate, WorkflowError> {
    let contexts = JobContexts {
        run,
        matrix: Value::Object(combination.clone()),
        strategy: json!({
            "fail-fast": strategy.fail_fast,
            "job-index": strategy.job_index,
            "job-total": strategy.job_total,
            "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
        }),
    };
    let evaluator = Evaluator { job, contexts: &contexts };
    let name = match &job.name {
        Some(name) => evaluator.text("name", name)?,
        None => String::new(),
    };
    let name = match name.trim() {
        "" if combination.is_empty() => job.id.clone(),
        "" => format!("{} {}", job.id, matrix_name(&combination)),
        _ => name,
    };
    let timeout = match &job.timeout_minutes {
        Some(text) => evaluator.minutes("timeout-minutes", text)?,
        None => DEFAULT_TIMEOUT_MINUTES,
    };
    let concurrency = match &job.concurrency {
        Some(concurrency) => {
            let group = evaluator.text("concurrency.group", &concurrency.group)?;
            if group.is_empty() {
                return Err(evaluator.refuse("concurrency.group", &"evaluates to an empty group"));
            }
            let cancel_in_progress = evaluator.flag("concurrency.cancel-in-progress", &concurrency.cancel_in_progress)?;
            Some(ConcurrencyGroup { group, cancel_in_progress })
        }
        None => None,
    };
    let (container, services) = containers(&evaluator, job)?;
    // Actions: environments.
    let environment = match &job.environment {
        Some(environment) => {
            let name = evaluator.text("environment", &environment.name)?;
            let name = name.trim();
            if !crate::spec::valid_environment_name(name) {
                return Err(evaluator.refuse("environment", &format!("`{}` is not an environment name", name.escape_debug())));
            }
            Some(name.to_owned())
        }
        None => None,
    };
    let template = JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: job.id.clone(),
        name,
        needs: job.needs.clone(),
        condition: job.condition.clone(),
        runs_on: evaluator.runs_on()?,
        matrix: combination,
        strategy,
        permissions: job.permissions.or(workflow.permissions).map(Into::into),
        concurrency,
        timeout_minutes: timeout.min(limits.max_timeout_minutes.max(1)),
        continue_on_error: job
            .continue_on_error
            .as_deref()
            .map(|text| evaluator.flag("continue-on-error", text))
            .transpose()?
            .unwrap_or(false),
        workflow_env: workflow.env.clone(),
        env: job.env.clone(),
        defaults: RunDefaults {
            shell: job.defaults.shell.clone().or_else(|| workflow.defaults.shell.clone()),
            working_directory: job.defaults.working_directory.clone().or_else(|| workflow.defaults.working_directory.clone()),
        },
        outputs: job.outputs.clone(),
        container,
        services,
        steps: job.steps.iter().map(step_template).collect(),
        environment,
        environment_url: job.environment.as_ref().and_then(|environment| environment.url.clone()),
        calls: Vec::new(),
        need_names: std::collections::BTreeMap::new(),
        deferred: false,
        default_ceiling: false,
    };
    template.validate().map_err(|problem| WorkflowError::plan(format!("job `{}`: {}", job.id, problem.message), Some(job.at)))?;
    Ok(template)
}

pub(crate) fn step_template(step: &Step) -> StepTemplate {
    StepTemplate {
        id: step.id.clone(),
        name: step.name.clone(),
        condition: step.condition.clone(),
        action: step.action.clone(),
        shell: step.shell.clone(),
        working_directory: step.working_directory.clone(),
        env: step.env.clone(),
        continue_on_error: step.continue_on_error.clone(),
        timeout_minutes: step.timeout_minutes.clone(),
    }
}
