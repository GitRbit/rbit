//! `rbit-runner exec`: plan and run a workflow from a local checkout, with no server
//! (§12.5.1).
//!
//! The workflow is parsed and planned with the same `rbit-workflow` code the server uses,
//! for the event `--event` names, and its jobs run with the same executor as
//! `rbit-runner run`, one after another in an order that respects `needs`. The parts the
//! server would supply come from the checkout instead:
//!
//! * the `github` context names the repository after the checkout's directory, its `HEAD`
//!   commit and branch, the event and an empty event payload;
//! * a job's `if` is evaluated here, against the results of the jobs it needs;
//! * secrets are the `--secret NAME` variables of this process's environment, and the task
//!   token is a placeholder no server accepts;
//! * the built-in checkout fetches the `HEAD` commit from the local repository, so changes
//!   that are not committed are not part of the run;
//! * every `runs-on` label runs on `--backend`: the host, or a container of
//!   `docker.io/library/ubuntu:24.04` unless the job names an image. A `runner.toml`, when
//!   there is one, supplies the container settings and the labels it defines.
//!
//! Log lines go to standard output, prefixed with the job's name and masked as on a
//! runner. With `--job`, only that job runs, and the jobs it needs count as successful.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, bail};
use rbit_workflow::{
    Deadline, Limits,
    expr::{Condition, ContextMap, Status},
    plan::RunContext,
    protocol::{Backend, TaskId, TaskResult},
    secret::Secret,
    spec::{
        CacheScopes, CheckoutSource, Contexts, JOB_SPEC_VERSION, JobResult, JobSpec, JobTemplate, MAX_SECRET_BYTES, Needed,
        RESERVED_NAME_PREFIXES,
    },
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    cli::{ExecArgs, ExecBackend},
    config::{self, Label, Loaded, RunnerConfig},
    job::JobExecutor,
    process,
    task::{Executor, Reporter},
};

/// The image of container labels `exec` makes up.
pub const DEFAULT_IMAGE: &str = "docker.io/library/ubuntu:24.04";

/// What `exec` knows of the local checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub root: PathBuf,
    pub repository: String,
    pub sha: String,
    pub reference: String,
}

pub async fn run(config_path: &Path, args: ExecArgs) -> anyhow::Result<()> {
    let source = std::fs::read(&args.workflow).with_context(|| format!("cannot read {}", args.workflow.display()))?;
    let workflow = rbit_workflow::parse(&source, &Limits::default(), &Deadline::after(Duration::from_secs(10)))
        .map_err(|error| anyhow::anyhow!("{}: {error}", args.workflow.display()))?;
    let checkout = local_checkout(&args.workflow).await?;
    let github = github_context(&checkout, &args.event, workflow.name.as_deref().unwrap_or("workflow"));
    let run = RunContext { github: github.clone(), inputs: Map::new(), vars: BTreeMap::new() };
    let mut templates = rbit_workflow::plan(&workflow, &run, &Limits::default(), &Deadline::after(Duration::from_secs(10)))
        .map_err(|error| anyhow::anyhow!("{}: {error}", args.workflow.display()))?;
    if let Some(only) = &args.job {
        templates.retain(|template| &template.job_id == only);
        if templates.is_empty() {
            bail!("the workflow has no job `{only}`");
        }
    }
    let secrets = secrets(&args.secrets, |name| std::env::var(name).ok())?;

    let work = tempfile_dir()?;
    let loaded = Arc::new(local_config(config_path, &templates, args.backend, &work));
    if let Some(warning) = config::host_warning(&loaded.config) {
        eprintln!("{}", warning.replace("WARNING: the label(s)", "note: the label(s)"));
    }
    let executor = JobExecutor::new(Arc::clone(&loaded));
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("cancelling: running steps are stopped, `always()` steps still run");
            cancel.cancel();
        });
    }

    let first_task = 1_000_000_000 + i64::from(getrandom::u32().unwrap_or(0) % 1_000_000);
    let mut results: BTreeMap<String, (JobResult, BTreeMap<String, String>)> = BTreeMap::new();
    let mut pending: Vec<JobTemplate> = templates;
    let mut summary = Vec::new();
    let mut task = first_task;
    while !pending.is_empty() {
        // A job is ready once no instance of anything it needs is still pending.
        let ready =
            pending.iter().position(|template| template.needs.iter().all(|need| !pending.iter().any(|other| &other.job_id == need)));
        let Some(ready) = ready else { bail!("the jobs' `needs` form a cycle") };
        let template = pending.remove(ready);
        let needs: BTreeMap<String, Needed> = template
            .needs
            .iter()
            .map(|need| {
                let (result, outputs) = results.get(need).cloned().unwrap_or((JobResult::Success, BTreeMap::new()));
                (need.clone(), Needed { result, outputs })
            })
            .collect();
        let name = template.name.clone();
        let (result, outputs) = if should_run(&template, &github, &needs)? {
            task += 1;
            let spec = job_spec(TaskId(task), template.clone(), github.clone(), needs, &secrets, &checkout);
            let (result, outputs) = execute(&executor, &loaded.work_dir, spec, &cancel).await;
            let result = match result {
                TaskResult::Success => JobResult::Success,
                // As on the server: a job allowed to fail counts as a success for its dependents.
                TaskResult::Failure if template.continue_on_error => JobResult::Success,
                TaskResult::Failure => JobResult::Failure,
                TaskResult::Cancelled => JobResult::Cancelled,
            };
            (result, outputs)
        } else {
            println!("[{name}] skipped: its `if` is false");
            (JobResult::Skipped, BTreeMap::new())
        };
        merge(&mut results, &template.job_id, result, outputs);
        summary.push((name, result));
    }
    let _ = std::fs::remove_dir_all(&work);

    println!();
    let mut failed = 0;
    for (name, result) in &summary {
        println!("{:>9}  {name}", crate::contexts::result_name(*result));
        failed += usize::from(matches!(result, JobResult::Failure | JobResult::Cancelled));
    }
    if failed > 0 {
        bail!("{failed} job(s) did not succeed");
    }
    Ok(())
}

/// Folds one finished job instance into `results`: a matrix job's result is the worst of
/// its instances, where skipped instances count only when all are, as on the server
/// (`aggregate`); their outputs are merged.
fn merge(
    results: &mut BTreeMap<String, (JobResult, BTreeMap<String, String>)>,
    job: &str,
    result: JobResult,
    outputs: BTreeMap<String, String>,
) {
    let entry = results.entry(job.to_owned()).or_insert((result, BTreeMap::new()));
    let rank = |result: JobResult| match result {
        JobResult::Skipped => 0,
        JobResult::Success => 1,
        JobResult::Cancelled => 2,
        JobResult::Failure => 3,
    };
    if rank(result) > rank(entry.0) {
        entry.0 = result;
    }
    entry.1.extend(outputs);
}

/// Whether the job's `if` (default `success()`) holds, given the results of its needs.
fn should_run(template: &JobTemplate, github: &Map<String, Value>, needs: &BTreeMap<String, Needed>) -> anyhow::Result<bool> {
    let condition = Condition::parse(template.condition.as_deref().unwrap_or_default())
        .map_err(|error| anyhow::anyhow!("job `{}`: `if`: {error}", template.job_id))?;
    let needs_value: Map<String, Value> = needs
        .iter()
        .map(|(id, needed)| (id.clone(), json!({"result": crate::contexts::result_name(needed.result), "outputs": needed.outputs})))
        .collect();
    let strategy = json!({
        "fail-fast": template.strategy.fail_fast,
        "job-index": template.strategy.job_index,
        "job-total": template.strategy.job_total,
        "max-parallel": template.strategy.max_parallel.unwrap_or(template.strategy.job_total),
    });
    let contexts = ContextMap::new()
        .with("github", github.clone())
        .with("inputs", Map::new())
        .with("vars", Map::new())
        .with("matrix", template.matrix.clone())
        .with("strategy", strategy)
        .with("needs", needs_value)
        .with_status(Status::of_needs(needs.values().map(|needed| needed.result)));
    condition.evaluate(&contexts).map_err(|error| anyhow::anyhow!("job `{}`: `if`: {error}", template.job_id))
}

async fn execute(
    executor: &JobExecutor,
    work_dir: &Path,
    spec: JobSpec,
    cancel: &CancellationToken,
) -> (TaskResult, BTreeMap<String, String>) {
    let name = spec.template.name.clone();
    let dir = crate::task::task_dir(work_dir, spec.task_id);
    let reporter = match Reporter::new(spec.task_id, &dir, spec.template.steps.len()) {
        Ok(reporter) => {
            let prefix = format!("[{name}] ");
            reporter.cancelled_by(cancel).echo_to(move |line| println!("{prefix}{line}"))
        }
        Err(error) => {
            println!("[{name}] cannot create the task directory: {error}");
            return (TaskResult::Failure, BTreeMap::new());
        }
    };
    for value in spec.secrets.values().chain([&spec.token]) {
        reporter.add_mask(value.expose());
    }
    let reporter = Arc::new(reporter);
    let result = executor.execute(spec, Arc::clone(&reporter)).await;
    let outputs = reporter.outputs();
    let _ = std::fs::remove_dir_all(&dir);
    (result, outputs)
}

/// The secrets `--secret NAME` asks for, read through `lookup`.
pub fn secrets(names: &[String], lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<BTreeMap<String, Secret>> {
    let mut secrets = BTreeMap::new();
    for name in names {
        let upper = name.to_ascii_uppercase();
        if upper.is_empty() || !upper.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
            bail!("`--secret {name}`: secret names are letters, digits and `_`");
        }
        if RESERVED_NAME_PREFIXES.iter().any(|prefix| upper.starts_with(prefix)) {
            bail!("`--secret {name}`: names starting with {} are reserved", RESERVED_NAME_PREFIXES.join(", "));
        }
        let value = lookup(name).with_context(|| format!("`--secret {name}`: the environment variable `{name}` is not set"))?;
        if value.len() > MAX_SECRET_BYTES {
            bail!("`--secret {name}` is larger than {MAX_SECRET_BYTES} bytes");
        }
        secrets.insert(upper, Secret::new(value));
    }
    Ok(secrets)
}

/// The repository around `workflow`, through `git`.
async fn local_checkout(workflow: &Path) -> anyhow::Result<Checkout> {
    let git = process::find_program("git").context("`exec` needs git to read the checkout")?;
    let dir = std::path::absolute(workflow)?.parent().map(Path::to_path_buf).unwrap_or_default();
    let ask = |args: &'static [&'static str]| {
        let (git, dir) = (git.clone(), dir.clone());
        async move {
            let mut command = tokio::process::Command::new(&git);
            command.arg("-C").arg(&dir).args(args);
            process::capture(command, None, Duration::from_secs(30)).await.ok().filter(|output| output.success).map(|output| output.stdout)
        }
    };
    let root = PathBuf::from(ask(&["rev-parse", "--show-toplevel"]).await.context("the workflow is not inside a Git checkout")?);
    let sha = ask(&["rev-parse", "HEAD"]).await.context("the checkout has no commit yet")?;
    let reference = ask(&["symbolic-ref", "-q", "HEAD"]).await.unwrap_or_else(|| "refs/heads/main".to_owned());
    let name = root.file_name().map_or_else(|| "repository".to_owned(), |name| name.to_string_lossy().into_owned());
    Ok(Checkout { root, repository: format!("local/{name}"), sha, reference })
}

/// The `github` context of a local run.
pub fn github_context(checkout: &Checkout, event: &str, workflow: &str) -> Map<String, Value> {
    let ref_name = checkout.reference.rsplit('/').next().unwrap_or_default().to_owned();
    let actor = ["USER", "USERNAME"].iter().find_map(|name| std::env::var(name).ok()).unwrap_or_else(|| "local".to_owned());
    let value = json!({
        "event_name": event,
        "event": {},
        "repository": checkout.repository,
        "repository_owner": "local",
        "sha": checkout.sha,
        "ref": checkout.reference,
        "ref_name": ref_name,
        "ref_type": if checkout.reference.starts_with("refs/tags/") { "tag" } else { "branch" },
        "actor": actor,
        "triggering_actor": actor,
        "workflow": workflow,
        "run_id": "1",
        "run_number": "1",
        "run_attempt": "1",
        "server_url": "http://localhost",
        "api_url": "http://localhost/api/v1",
    });
    value.as_object().cloned().unwrap_or_default()
}

/// A task for one planned job.
fn job_spec(
    task_id: TaskId,
    template: JobTemplate,
    github: Map<String, Value>,
    needs: BTreeMap<String, Needed>,
    secrets: &BTreeMap<String, Secret>,
    checkout: &Checkout,
) -> JobSpec {
    // Shaped like a task token so masking and `secrets.GITHUB_TOKEN` behave as on a runner;
    // no server knows it.
    let token = format!("rbat_{}", (crate::client::idempotency_key() + &crate::client::idempotency_key())[..43].to_owned());
    let root = checkout.root.to_string_lossy().replace('\\', "/");
    let url = if root.starts_with('/') { format!("file://{root}") } else { format!("file:///{root}") };
    let scope = checkout.reference.clone();
    JobSpec {
        version: JOB_SPEC_VERSION,
        task_id,
        template,
        contexts: Contexts { github, inputs: Map::new(), vars: BTreeMap::new(), needs },
        secrets: secrets.clone(),
        token: Secret::new(token),
        checkout: CheckoutSource {
            url,
            repository: checkout.repository.clone(),
            sha: checkout.sha.clone(),
            reference: checkout.reference.clone(),
        },
        cache: CacheScopes { repository_id: 0, read: vec![scope.clone()], write: scope },
    }
}

/// The runner configuration of a local run: `runner.toml` when it loads, with every label
/// the jobs ask for that it lacks added on `backend`.
fn local_config(config_path: &Path, templates: &[JobTemplate], backend: ExecBackend, work: &Path) -> Loaded {
    let mut config = config::load(config_path).map_or_else(
        |_| RunnerConfig {
            url: "https://localhost".into(),
            insecure_http: false,
            name: "local".into(),
            capacity: 1,
            work_dir: work.to_path_buf(),
            shutdown_grace_seconds: 600,
            labels: Vec::new(),
            container: config::ContainerConfig::default(),
            cache: config::CacheConfig::default(),
        },
        |loaded| loaded.config,
    );
    for label in templates.iter().flat_map(|template| template.runs_on.iter()) {
        if !config.labels.iter().any(|known| &known.name == label) {
            config.labels.push(match backend {
                ExecBackend::Host => Label { name: label.clone(), backend: Backend::Host, image: None },
                ExecBackend::Container => Label { name: label.clone(), backend: Backend::Container, image: Some(DEFAULT_IMAGE.into()) },
            });
        }
    }
    // Configured labels follow `--backend` too: a local run runs where it was asked to.
    for label in &mut config.labels {
        match backend {
            ExecBackend::Host => {
                label.backend = Backend::Host;
                label.image = None;
            }
            ExecBackend::Container if label.backend == Backend::Host => {
                label.backend = Backend::Container;
                label.image = Some(DEFAULT_IMAGE.into());
            }
            ExecBackend::Container => {}
        }
    }
    Loaded { config, work_dir: work.to_path_buf(), credential_path: work.join(config::CREDENTIAL_FILE) }
}

/// A fresh directory for the run's tasks and cache.
fn tempfile_dir() -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("rbit-runner-exec-{}", crate::client::idempotency_key()));
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_come_from_the_environment() {
        let env = |name: &str| (name == "DEPLOY_KEY" || name == "token").then(|| "value".to_owned());
        let secrets = secrets(&["DEPLOY_KEY".into(), "token".into()], env).unwrap();
        assert_eq!(secrets.keys().collect::<Vec<_>>(), ["DEPLOY_KEY", "TOKEN"]);
        assert!(super::secrets(&["MISSING".into()], env).unwrap_err().to_string().contains("not set"));
        assert!(super::secrets(&["GITHUB_TOKEN".into()], env).unwrap_err().to_string().contains("reserved"));
        assert!(super::secrets(&["a-b".into()], env).is_err());
    }

    #[test]
    fn labels_follow_the_requested_backend() {
        let dir = tempfile::tempdir().unwrap();
        let mut template = crate::testing::job_spec(1, 1).template;
        template.runs_on = vec!["ubuntu-latest".into(), "gpu".into()];
        let host = local_config(&dir.path().join("none.toml"), std::slice::from_ref(&template), ExecBackend::Host, dir.path());
        assert!(host.config.labels.iter().all(|label| label.backend == Backend::Host && label.image.is_none()));
        let container = local_config(&dir.path().join("none.toml"), &[template], ExecBackend::Container, dir.path());
        assert!(container.config.labels.iter().all(|label| label.image.as_deref() == Some(DEFAULT_IMAGE)));
        container.config.validate().unwrap();
    }

    #[test]
    fn job_results_merge_across_a_matrix() {
        let mut results = BTreeMap::new();
        merge(&mut results, "test", JobResult::Success, [("a".to_owned(), "1".to_owned())].into());
        merge(&mut results, "test", JobResult::Failure, [("b".to_owned(), "2".to_owned())].into());
        merge(&mut results, "test", JobResult::Success, BTreeMap::new());
        assert_eq!(results["test"].0, JobResult::Failure);
        assert_eq!(results["test"].1.len(), 2);

        merge(&mut results, "lint", JobResult::Success, BTreeMap::new());
        merge(&mut results, "lint", JobResult::Skipped, BTreeMap::new());
        merge(&mut results, "docs", JobResult::Skipped, BTreeMap::new());
        merge(&mut results, "docs", JobResult::Success, BTreeMap::new());
        merge(&mut results, "bench", JobResult::Skipped, BTreeMap::new());
        merge(&mut results, "bench", JobResult::Skipped, BTreeMap::new());
        let result = |job: &str| results[job].0;
        assert_eq!((result("lint"), result("docs"), result("bench")), (JobResult::Success, JobResult::Success, JobResult::Skipped));
    }

    #[tokio::test]
    async fn a_workflow_runs_from_a_local_checkout() {
        let (Some(git), Some(_)) = (process::find_program("git"), process::find_program(if cfg!(windows) { "powershell" } else { "sh" }))
        else {
            return;
        };
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new(&git)
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main"])
                .args(args)
                .current_dir(repo.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        };
        git(&["init", "--quiet"]);
        std::fs::create_dir_all(repo.path().join(".rbit/workflows")).unwrap();
        std::fs::write(repo.path().join("hello.txt"), "hello from the checkout").unwrap();
        let workflow = "\
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    outputs:
      greeting: ${{ steps.say.outputs.greeting }}
    steps:
      - uses: actions/checkout@v4
      - id: say
        run: echo \"greeting=hi\" >> $env:GITHUB_OUTPUT
        shell: SHELL
  test:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - run: echo \"${{ needs.build.outputs.greeting }} ${{ secrets.LOCAL_SECRET }}\"
  never:
    needs: build
    if: failure()
    runs-on: ubuntu-latest
    steps:
      - run: echo never
";
        let (shell, output_ref) = if cfg!(windows) { ("powershell", "$env:GITHUB_OUTPUT") } else { ("sh", "\"$GITHUB_OUTPUT\"") };
        let workflow = workflow.replace("SHELL", shell).replace("$env:GITHUB_OUTPUT", output_ref);
        let path = repo.path().join(".rbit/workflows/ci.yml");
        std::fs::write(&path, workflow).unwrap();
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "ci"]);

        let checkout = local_checkout(&path).await.unwrap();
        assert_eq!(checkout.reference, "refs/heads/main");
        assert!(checkout.repository.starts_with("local/"));
        let args = ExecArgs { workflow: path.clone(), job: None, event: "push".into(), backend: ExecBackend::Host, secrets: Vec::new() };
        let finished = run(&repo.path().join("runner.toml"), args).await;
        assert!(finished.is_ok(), "{finished:?}");

        // A failing job fails the command.
        std::fs::write(&path, "on: push\njobs:\n  broken:\n    runs-on: x\n    steps:\n      - run: exit 3\n").unwrap();
        let args =
            ExecArgs { workflow: path, job: Some("broken".into()), event: "push".into(), backend: ExecBackend::Host, secrets: Vec::new() };
        let error = run(&repo.path().join("runner.toml"), args).await.unwrap_err().to_string();
        assert!(error.contains("1 job(s) did not succeed"), "{error}");
    }
}
