//! What a job's log says before its first step and after its last, which the job page
//! shows as "Set up job" and "Complete job" (GitHub's names): the runner, its version and
//! labels, the operating system and backend, the container images, the job's settings and
//! token permissions, the actions the steps use with the commits their references name, and
//! at the end the result. Also why a step was skipped. These lines are English, like the
//! rest of the log.

use std::collections::BTreeSet;

use rbit_workflow::{
    protocol::{Backend as BackendKind, TaskResult},
    spec::{ActionSource, Env, StepAction, StepTemplate, TokenPermissions},
};

use crate::{
    backend::{Backend, runner_arch},
    config::Loaded,
    job::{Job, Phase},
    process,
    steps::action,
};

pub const fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Container => "container",
        BackendKind::Host => "host — no isolation",
    }
}

fn permissions(permissions: Option<&TokenPermissions>) -> String {
    let Some(permissions) = permissions else { return "the repository's default".to_owned() };
    let access = |access| format!("{access:?}").to_ascii_lowercase();
    format!(
        "contents: {}, issues: {}, pull-requests: {}",
        access(permissions.contents),
        access(permissions.issues),
        access(permissions.pull_requests)
    )
}

fn names(env: &Env) -> String {
    env.keys().cloned().collect::<Vec<_>>().join(", ")
}

/// Logs what the job runs on and with, and resolves the references of the actions its
/// steps use, so the commits the steps fetch are the ones named here.
pub async fn describe(job: &mut Job, loaded: &Loaded) {
    let template = job.spec.template.clone();
    let log = |line: &str| job.reporter.log(line);
    log(&format!("Current runner version: '{}'", env!("CARGO_PKG_VERSION")));
    log(&format!("Runner name: '{}'", loaded.config.name));
    let labels: Vec<&str> = loaded.config.labels.iter().map(|label| label.name.as_str()).collect();
    log(&format!("Runner labels: {}", labels.join(", ")));
    log(&format!("Runs on: {} (label `{}`, {} backend)", template.runs_on.join(", "), job.label.name, backend_name(job.label.backend)));
    log("##[group]Operating system");
    log(&format!("{} {}", job.backend.os(), runner_arch(std::env::consts::ARCH)));
    log("##[endgroup]");
    if matches!(job.backend, Backend::Container(_)) {
        log("##[group]Container images");
        let image =
            template.container.as_ref().map_or_else(|| job.label.image.clone().unwrap_or_default(), |container| container.image.clone());
        log(&format!("Job: {image}"));
        for service in &template.services {
            log(&format!("Service `{}`: {}", service.id, service.image));
        }
        log("##[endgroup]");
    }
    log("##[group]Job settings");
    log(&format!("Timeout: {} minutes", template.timeout_minutes));
    log(&format!("Token permissions: {}", permissions(template.permissions.as_ref())));
    if let Some(environment) = &template.environment {
        log(&format!("Environment: {environment}"));
    }
    if template.continue_on_error {
        log("Continue on error: yes");
    }
    for (key, value) in &template.matrix {
        log(&format!("Matrix {key}: {}", rbit_workflow::expr::to_text(value)));
    }
    if !template.env.is_empty() || !template.workflow_env.is_empty() {
        log(&format!(
            "Environment variables: {}",
            names(&template.workflow_env)
                .split(", ")
                .chain(names(&template.env).split(", "))
                .filter(|name| !name.is_empty())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    log("##[endgroup]");
    resolve_actions(job, &template.steps).await;
    job.reporter.log(&format!("Complete job name: {}", template.name));
}

/// Resolves every repository action the steps name, once each, into `job.resolved`.
async fn resolve_actions(job: &mut Job, steps: &[StepTemplate]) {
    let mut seen = BTreeSet::new();
    let mut wanted = Vec::new();
    for step in steps {
        let StepAction::Action(step) = &step.action else { continue };
        if action::refused(&job.actions.allow, &step.source).is_some() || job.spec.actions_policy.refuses(&step.source).is_some() {
            continue;
        }
        match &step.source {
            ActionSource::Repository { owner, repo, reference, .. } if seen.insert(format!("{owner}/{repo}@{reference}")) => {
                wanted.push((owner.clone(), repo.clone(), reference.clone()));
            }
            ActionSource::Docker { image } if seen.insert(format!("docker://{image}")) => {
                job.reporter.log(&format!("Docker action image '{image}'"));
            }
            _ => {}
        }
    }
    if wanted.is_empty() {
        return;
    }
    job.reporter.log("Prepare all required actions");
    for (owner, repo, reference) in wanted {
        let key = format!("{owner}/{repo}@{reference}");
        match lookup(job, &owner, &repo, &reference).await {
            Ok(sha) => {
                job.reporter.log(&format!("Resolved action repository '{key}' (SHA:{sha})"));
                job.resolved.insert(key, sha);
            }
            Err(error) => job.reporter.log(&format!("##[warning]Cannot resolve '{key}' yet; the step tries again: {error}")),
        }
    }
}

async fn lookup(job: &Job, owner: &str, repo: &str, reference: &str) -> Result<String, String> {
    if action::is_commit(reference) {
        return Ok(reference.to_ascii_lowercase());
    }
    let url = format!("{}/{owner}/{repo}", job.actions.url.trim_end_matches('/'));
    let git = process::find_program("git").ok_or("fetching actions needs `git` on the runner host")?;
    let env = action::git_env(&url, job.private_dir().ok().as_deref());
    action::resolve(&git, &env, &url, reference).await
}

/// The commit `owner/repo@reference` names: as resolved while the job was set up, or now.
pub async fn sha(
    job: &Job,
    git: &std::path::Path,
    env: &std::collections::BTreeMap<String, String>,
    url: &str,
    name: &str,
    reference: &str,
) -> Result<String, String> {
    match job.resolved.get(&format!("{name}@{reference}")) {
        Some(sha) => Ok(sha.clone()),
        None if action::is_commit(reference) => Ok(reference.to_ascii_lowercase()),
        None => action::resolve(git, env, url, reference).await,
    }
}

/// Step `index` is skipped: its log says why, as GitHub's does.
pub fn skip(job: &Job, index: usize, step: &StepTemplate) {
    let why = match (step.condition.as_deref().map(str::trim).filter(|text| !text.is_empty()), job.phase) {
        (Some(condition), _) => format!("This step was skipped because its condition `{condition}` was false"),
        (None, Phase::Cancelled) => "This step was skipped because the job was cancelled".to_owned(),
        (None, _) => "This step was skipped because an earlier step failed".to_owned(),
    };
    job.reporter.skip_step(index, &why);
}

/// The job's last lines: what is cleaned up, and how it ended.
pub fn complete(job: &Job, result: TaskResult) {
    if matches!(job.backend, Backend::Container(_)) {
        job.reporter.log("Stop and remove the job's containers");
    }
    let result = match result {
        TaskResult::Success => "success",
        TaskResult::Failure => "failure",
        TaskResult::Cancelled => "cancelled",
    };
    job.reporter.log(&format!("Job completed with result: {result}"));
}

#[cfg(test)]
mod tests {
    use crate::job::tests::{execute, host_job, step};
    use rbit_workflow::protocol::{StepStatus, TaskResult};

    #[tokio::test]
    async fn the_log_says_what_the_job_runs_on_how_it_ended_and_why_steps_were_skipped() {
        let mut skipped = step("echo never");
        skipped.condition = Some("false".into());
        let (result, reporter, _work) = execute(host_job(vec![step("exit 3"), step("echo after"), skipped])).await;
        assert_eq!(result, TaskResult::Failure);
        let lines = reporter.lines();
        let state = reporter.state();
        assert!(lines[0].starts_with("Current runner version: '"), "{lines:?}");
        for expected in ["Runner name: '", "Runner labels: ", "##[group]Operating system", "Timeout: ", "Complete job name: "] {
            assert!(lines.iter().any(|line| line.starts_with(expected)), "{expected} in {lines:?}");
        }
        assert!(lines.iter().any(|line| line == "Token permissions: the repository's default"));
        let setup_end = lines.iter().position(|line| line.starts_with("Complete job name: ")).unwrap();
        assert_eq!(usize::try_from(state.steps[0].log_index).unwrap(), setup_end + 1, "set-up lines come before the first step");
        assert!(lines.iter().any(|line| line.starts_with("##[error]Process completed with exit code ")), "{lines:?}");
        for (index, why) in [
            (1, "This step was skipped because an earlier step failed"),
            (2, "This step was skipped because its condition `false` was false"),
        ] {
            let step = state.steps[index];
            assert_eq!((step.status, step.started_at, step.log_length), (StepStatus::Skipped, None, 1));
            assert_eq!(lines[usize::try_from(step.log_index).unwrap()], why);
        }
        assert_eq!(lines.last().map(String::as_str), Some("Job completed with result: failure"));
    }
}
