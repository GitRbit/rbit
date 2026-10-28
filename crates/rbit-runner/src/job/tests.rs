//! Jobs on the host backend, with whichever shells this machine has.

use std::{sync::Arc, time::Duration};

use rbit_workflow::{
    protocol::{StepStatus, TaskResult},
    secret::Secret,
    spec::{Container, Env, JobSpec, StepAction, StepTemplate},
};
use serde_json::json;

use super::*;
use crate::{
    config::{self, Loaded},
    process::find_program,
    task::{Executor, Reporter},
    testing::job_spec,
};

const SECRET: &str = "hunter2-secret-value";

fn loaded(work: &std::path::Path) -> Loaded {
    Loaded { config: config::tests::config(), work_dir: work.to_path_buf(), credential_path: work.join(config::CREDENTIAL_FILE) }
}

pub(crate) fn step(script: &str) -> StepTemplate {
    StepTemplate {
        id: None,
        name: None,
        condition: None,
        action: StepAction::Run(script.to_owned()),
        shell: None,
        working_directory: None,
        env: Env::new(),
        continue_on_error: None,
        timeout_minutes: None,
    }
}

pub(crate) fn host_job(steps: Vec<StepTemplate>) -> JobSpec {
    let mut spec = job_spec(1, 1);
    spec.template.runs_on = vec!["self-hosted".into()];
    spec.template.steps = steps;
    spec.secrets.insert("MY_SECRET".into(), Secret::new(SECRET));
    spec.contexts.github =
        json!({"repository": "alice/app", "sha": "0123456789abcdef0123456789abcdef01234567", "run_id": 9}).as_object().cloned().unwrap();
    spec
}

/// Runs `spec` as the poller would, and returns the result and the reporter.
pub(crate) async fn execute(spec: JobSpec) -> (TaskResult, Arc<Reporter>, tempfile::TempDir) {
    execute_with(spec, |_| {}, |_| {}).await
}

async fn execute_with(
    spec: JobSpec,
    configure: impl FnOnce(&mut Loaded),
    during: impl FnOnce(Arc<Reporter>),
) -> (TaskResult, Arc<Reporter>, tempfile::TempDir) {
    let work = tempfile::tempdir().unwrap();
    let mut loaded = loaded(work.path());
    configure(&mut loaded);
    let reporter =
        Arc::new(Reporter::new(spec.task_id, &crate::task::task_dir(work.path(), spec.task_id), spec.template.steps.len()).unwrap());
    for value in spec.secrets.values().chain([&spec.token]) {
        reporter.add_mask(value.expose());
    }
    during(Arc::clone(&reporter));
    let executor = JobExecutor::new(Arc::new(loaded));
    let result = tokio::time::timeout(Duration::from_secs(60), executor.execute(spec, Arc::clone(&reporter))).await.unwrap();
    (result, reporter, work)
}

fn statuses(reporter: &Reporter) -> Vec<StepStatus> {
    reporter.state().steps.iter().map(|step| step.status).collect()
}

fn with(mut step: StepTemplate, change: impl FnOnce(&mut StepTemplate)) -> StepTemplate {
    change(&mut step);
    step
}

#[tokio::test]
async fn steps_share_env_files_outputs_and_masks() {
    if find_program("sh").is_none() {
        return;
    }
    let sh = |script: &str| with(step(script), |step| step.shell = Some("sh".into()));
    let steps = vec![
        with(
            sh(concat!(
                "echo \"version=1.2.3\" >> \"$GITHUB_OUTPUT\"\n",
                "echo \"GREETING=hello\" >> \"$RBIT_ENV\"\n",
                "echo \"LD_PRELOAD=/evil.so\" >> \"$GITHUB_ENV\"\n",
                "printf 'notes<<EOF\\nline one\\nline two\\nEOF\\n' >> \"$GITHUB_OUTPUT\"\n",
                "echo \"$RUNNER_TEMP/rbit-extra-tools\" >> \"$GITHUB_PATH\"\n",
                "echo \"::add-mask::masked-later\"\n",
                "echo \"secret is ${{ secrets.MY_SECRET }}\"\n",
                "echo \"::warning::look here\"\n",
                "echo \"# Released ${{ secrets.MY_SECRET }}\" >> \"$GITHUB_STEP_SUMMARY\"\n",
            )),
            |step| step.id = Some("first".into()),
        ),
        sh(concat!(
            "echo \"$GREETING ${{ steps.first.outputs.version }} ${{ env.GREETING }} masked-later\"\n",
            "echo \"path=$PATH\"\n",
            "echo \"preload=[${LD_PRELOAD:-}] token=[${GITHUB_TOKEN:-none}] ci=$CI/$GITHUB_ACTIONS/$RBIT_ACTIONS sha=$GITHUB_SHA\"\n",
        )),
        with(sh("exit 3"), |step| {
            step.id = Some("broken".into());
            step.continue_on_error = Some("true".into());
        }),
        with(sh("echo not after a tolerated failure"), |step| step.condition = Some("failure()".into())),
        with(sh("echo \"mapped=$TOKEN\""), |step| _ = step.env.insert("TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())),
        with(sh("exit 1"), |step| step.condition = Some("${{ always() }}".into())),
        sh("echo never"),
        with(sh("echo \"after failure: ${{ job.status }}\""), |step| step.condition = Some("failure()".into())),
    ];
    let mut spec = host_job(steps);
    spec.template.outputs = [
        ("version".to_owned(), "${{ steps.first.outputs.version }}".to_owned()),
        ("broken".to_owned(), "${{ steps.broken.outcome }}/${{ steps.broken.conclusion }}".to_owned()),
        ("leak".to_owned(), "${{ secrets.MY_SECRET }}".to_owned()),
    ]
    .into();
    let (result, reporter, _work) = execute(spec).await;
    let log = reporter.lines();
    let text = log.join("\n");

    assert_eq!(result, TaskResult::Failure, "{text}");
    let (success, failure, skipped) = (StepStatus::Success, StepStatus::Failure, StepStatus::Skipped);
    assert_eq!(statuses(&reporter), [success, success, failure, skipped, success, failure, skipped, success], "{text}");
    assert!(text.contains("hello 1.2.3 hello ***"), "{text}");
    assert!(text.contains("secret is ***") && !text.contains(SECRET), "{text}");
    assert!(text.contains("##[warning]look here"));
    assert!(text.contains("GITHUB_ENV cannot set `LD_PRELOAD`"));
    assert!(text.contains("preload=[] token=[none] ci=true/true/true sha=0123456789abcdef"), "{text}");
    assert!(log.iter().any(|line| line.starts_with("path=") && line.contains("rbit-extra-tools")), "{text}");
    assert!(text.contains("mapped=***") && !text.contains(crate::testing::TOKEN), "{text}");
    assert!(text.contains("Process completed with exit code 3."));
    assert!(text.contains("after failure: failure"));
    let outputs = reporter.state().outputs;
    assert_eq!((outputs["version"].as_str(), outputs["broken"].as_str(), outputs["leak"].as_str()), ("1.2.3", "failure/success", "***"));
    // Actions: step summaries. Sent masked, by the step that wrote it.
    let summaries = reporter.state().summaries;
    assert_eq!(summaries, std::collections::BTreeMap::from([(0, "# Released ***\n".to_owned())]), "{text}");
}

#[tokio::test]
async fn a_path_set_in_env_is_the_base_of_the_steps_path() {
    if find_program("sh").is_none() {
        return;
    }
    let sh = |script: &str| with(step(script), |step| step.shell = Some("sh".into()));
    let separator = if cfg!(windows) { ";" } else { ":" };
    let own = format!("rbit-own-tools{separator}{}", std::env::var("PATH").unwrap());
    let steps = vec![
        sh("echo \"$RUNNER_TEMP/rbit-extra-tools\" >> \"$GITHUB_PATH\""),
        with(sh("echo \"path=$PATH\""), |step| _ = step.env.insert("PATH".into(), own.clone())),
        sh("echo \"plain=$PATH\""),
    ];
    let (result, reporter, _work) = execute(host_job(steps)).await;
    let log = reporter.lines();
    let text = log.join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    let path = log.iter().find_map(|line| line.strip_prefix("path=")).unwrap();
    let (extra, own) = (path.find("rbit-extra-tools"), path.find("rbit-own-tools"));
    assert!(extra.is_some() && extra < own, "GITHUB_PATH entries come first, then the step's own PATH: {text}");
    let plain = log.iter().find_map(|line| line.strip_prefix("plain=")).unwrap();
    assert!(plain.contains("rbit-extra-tools") && !plain.contains("rbit-own-tools"), "{text}");
}

#[tokio::test]
async fn bash_runs_with_pipefail_and_the_working_directory() {
    if find_program("bash").is_none() {
        return;
    }
    let bash = |script: &str| with(step(script), |step| step.shell = Some("bash".into()));
    let steps = vec![
        bash("mkdir -p sub/dir && echo made"),
        with(bash("pwd; false | true"), |step| step.working_directory = Some("sub/dir".into())),
        bash("echo after"),
    ];
    let (result, reporter, _work) = execute(host_job(steps)).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Failure, "pipefail makes `false | true` fail: {text}");
    assert_eq!(statuses(&reporter), [StepStatus::Success, StepStatus::Failure, StepStatus::Skipped], "{text}");
    assert!(reporter.lines().iter().any(|line| line.ends_with("sub/dir") || line.ends_with("sub\\dir")), "{text}");
}

#[cfg(windows)]
#[tokio::test]
async fn powershell_and_cmd_run_on_windows() {
    let shell = |name: &str, script: &str| with(step(script), |step| step.shell = Some(name.into()));
    let steps = vec![
        with(
            shell(
                "powershell",
                "Write-Output \"ps says ${{ github.repository }}\"\n\"value=from-ps\" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8",
            ),
            |step| step.id = Some("ps".into()),
        ),
        with(shell("cmd", "echo result=from-cmd>> \"%GITHUB_OUTPUT%\"\necho cmd says %CI%"), |step| step.id = Some("cmd".into())),
        shell("powershell", "Write-Output \"${{ steps.ps.outputs.value }} ${{ steps.cmd.outputs.result }}\""),
        shell("cmd", "exit /b 4"),
    ];
    let (result, reporter, _work) = execute(host_job(steps)).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Failure, "{text}");
    assert!(text.contains("ps says alice/app") && text.contains("cmd says true"), "{text}");
    assert!(text.contains("from-ps from-cmd"), "{text}");
    assert!(text.contains("exit code 4"), "{text}");
}

#[tokio::test]
async fn a_cancelled_job_stops_its_step_and_runs_only_always_steps() {
    if find_program("sh").is_none() {
        return;
    }
    let sh = |script: &str| with(step(script), |step| step.shell = Some("sh".into()));
    let steps = vec![
        sh("echo started; sleep 15"),
        sh("echo not run"),
        with(sh("echo cleanup ran"), |step| step.condition = Some("always()".into())),
    ];
    let started = tokio::time::Instant::now();
    let (result, reporter, _work) = execute_with(
        host_job(steps),
        |_| {},
        |reporter| {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                reporter.cancellation().cancel();
            });
        },
    )
    .await;
    // Within the grace period: `sh` and `sleep` end on the polite signal, and where that
    // cannot be sent (Windows' `taskkill` without `/F`), the forceful one goes at once.
    assert!(started.elapsed() < crate::process::STOP_GRACE, "{:?}", started.elapsed());
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Cancelled, "{text}");
    assert_eq!(statuses(&reporter), [StepStatus::Cancelled, StepStatus::Skipped, StepStatus::Success], "{text}");
    assert!(text.contains("cleanup ran") && !text.contains("not run"), "{text}");
}

#[tokio::test]
async fn jobs_the_label_cannot_run_fail_before_any_step() {
    let mut spec = host_job(vec![step("echo hi")]);
    spec.template.container = Some(Container { image: "alpine".into(), env: Env::new(), credentials: None });
    let (result, reporter, _work) = execute(spec).await;
    assert_eq!(result, TaskResult::Failure);
    assert!(reporter.lines()[0].contains("use a container label"), "{:?}", reporter.lines());
    assert_eq!(statuses(&reporter), [StepStatus::Skipped]);

    let mut spec = host_job(vec![step("echo hi")]);
    spec.template.runs_on = vec!["windows-arm".into()];
    let (result, reporter, _work) = execute(spec).await;
    assert_eq!(result, TaskResult::Failure);
    assert!(reporter.lines()[0].contains("none of the labels `windows-arm`"));

    let mut spec = host_job(vec![with(step("echo"), |step| step.condition = Some("${{ nonsense( }}".into()))]);
    spec.template.workflow_env.insert("A".into(), "${{ github.repository }}".into());
    let (result, reporter, _work) = execute(spec).await;
    assert_eq!((result, statuses(&reporter)), (TaskResult::Failure, vec![StepStatus::Failure]));

    // A name a container engine's env file could misread never gets that far.
    let mut spec = host_job(vec![step("echo hi")]);
    spec.template.env.insert("AWS_SECRET_ACCESS_KEY\nX".into(), "x".into());
    let (result, reporter, _work) = execute(spec).await;
    assert_eq!((result, statuses(&reporter)), (TaskResult::Failure, vec![StepStatus::Skipped]));
    assert!(reporter.lines().iter().any(|line| line.contains("not a valid variable name")), "{:?}", reporter.lines());
}

#[test]
fn the_task_environment_names_the_run_but_not_the_token() {
    let github = json!({"repository": "alice/app", "sha": "abc", "run_number": 4, "event": {"x": 1}, "token": "rbat_x"});
    let view = crate::backend::View { workspace: "/w".into(), temp: "/t".into(), files: "/f".into() };
    let env = task_env(github.as_object().unwrap(), &view, "build-1", "Linux", "X64");
    for (name, value) in [
        ("CI", "true"),
        ("GITHUB_ACTIONS", "true"),
        ("RBIT_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "alice/app"),
        ("RBIT_REPOSITORY", "alice/app"),
        ("GITHUB_RUN_NUMBER", "4"),
        ("RUNNER_TEMP", "/t"),
        ("RUNNER_NAME", "build-1"),
        ("RUNNER_OS", "Linux"),
    ] {
        assert_eq!(env.get(name).map(String::as_str), Some(value), "{name}");
    }
    assert!(!env.contains_key("GITHUB_EVENT") && !env.contains_key("GITHUB_TOKEN") && !env.values().any(|value| value.contains("rbat_")));
}

#[tokio::test]
async fn container_jobs_need_docker_or_podman() {
    let mut spec = host_job(vec![step("echo hi")]);
    spec.template.runs_on = vec!["ubuntu-latest".into()];
    let (result, reporter, _work) =
        execute_with(spec, |loaded| loaded.config.container.program = "rbit-no-such-engine".into(), |_| {}).await;
    assert_eq!(result, TaskResult::Failure);
    assert!(reporter.lines()[0].contains("`rbit-no-such-engine` is not on PATH"), "{:?}", reporter.lines());
}

/// A stand-in for the container CLI: it records its arguments, answers the probe, and
/// "runs" each step by printing its environment file's names.
#[cfg(unix)]
#[tokio::test]
async fn container_jobs_drive_the_cli() {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = tempfile::tempdir().unwrap();
    let calls = bin.path().join("calls");
    let fake = bin.path().join("docker");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\ncase \"$1 $3\" in\n  'exec sh') echo 'PATH=/usr/local/bin:/usr/bin:/bin'; echo sh; echo bash ;;\n  exec*) echo step ran ;;\nesac\nexit 0\n",
            calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let cache = with(step(""), |step| {
        step.action = StepAction::Uses(rbit_workflow::spec::Builtin {
            uses: "rbit/cache@v1".into(),
            kind: rbit_workflow::spec::BuiltinKind::Cache,
            version: 1,
            with: [("key", "deps"), ("path", "target")].into_iter().map(|(key, value)| (key.to_owned(), value.to_owned())).collect(),
        });
    });
    let mut spec = host_job(vec![
        with(step("echo ${{ secrets.MY_SECRET }}"), |step| {
            step.env.insert("SECRET_ENV".into(), "${{ secrets.MY_SECRET }}".into());
        }),
        cache,
    ]);
    spec.template.runs_on = vec!["ubuntu-latest".into()];
    spec.template.container = Some(Container {
        image: "docker.io/library/rust:1.94".into(),
        env: Env::from([("IN_CONTAINER".into(), "${{ secrets.MY_SECRET }}".into())]),
        credentials: None,
    });
    spec.template.services = vec![rbit_workflow::spec::Service {
        id: "postgres".into(),
        image: "postgres:17".into(),
        env: Env::from([("POSTGRES_PASSWORD".into(), "pw".into())]),
        credentials: None,
    }];
    let program = fake.display().to_string();
    let (result, reporter, _work) = execute_with(spec, |loaded| loaded.config.container.program = program, |_| {}).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(text.contains("step ran"), "{text}");

    let calls = std::fs::read_to_string(&calls).unwrap();
    let lines: Vec<&str> = calls.lines().collect();
    let position =
        |prefix: &str| lines.iter().position(|line| line.starts_with(prefix)).unwrap_or_else(|| panic!("no `{prefix}` in {calls}"));
    let order = [
        "network create",
        "pull postgres:17",
        "run --detach --name rbit-1-postgres",
        "pull docker.io/library/rust:1.94",
        "create --name rbit-1-job",
        "start rbit-1-job",
        "exec rbit-1-job sh -c",
        "info --format",
        "exec --env-file",
        // The cache step works on the workspace from the host, with the container paused
        // and the files handed back to the runner's user first.
        "pause rbit-1-job",
        "run --rm --label rbit.task=1",
        "unpause rbit-1-job",
        "rm --force rbit-1-postgres rbit-1-job",
        "network rm rbit-1",
    ];
    let positions: Vec<usize> = order.iter().map(|prefix| position(prefix)).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{calls}");
    assert!(!calls.contains(SECRET), "no secret in any argument: {calls}");
    assert_eq!(lines.iter().filter(|line| **line == "pause rbit-1-job").count(), 2, "the step, then the post-job save: {calls}");
    // Before each of the two pauses' work, and once the containers are gone.
    let reclaims: Vec<usize> = (0..lines.len()).filter(|&at| lines[at].starts_with("run --rm --label rbit.task=1")).collect();
    assert_eq!(reclaims.len(), 3, "{calls}");
    assert!(reclaims[2] > position("rm --force") && reclaims[2] < position("network rm"), "{calls}");
    assert!(lines[reclaims[0]].contains(&format!("--entrypoint sh {}", crate::backend::container::HELPER_IMAGE)), "{calls}");
    assert!(
        lines[position("exec --env-file")]
            .contains("--workdir /__w/app rbit-1-job bash --noprofile --norc -eo pipefail /__w/_temp/step-0-"),
        "{calls}"
    );
}

#[tokio::test]
async fn a_step_that_runs_too_long_is_stopped_and_fails() {
    if find_program("sh").is_none() {
        return;
    }
    let sh = |script: &str| with(step(script), |step| step.shell = Some("sh".into()));
    let steps = vec![
        with(sh("echo before; sleep 14; echo after"), |step| step.timeout_minutes = Some("${{ 0.02 }}".into())),
        sh("echo skipped after a failure"),
        with(sh("echo always ran"), |step| step.condition = Some("always()".into())),
        with(sh("echo x"), |step| step.timeout_minutes = Some("soon".into())),
    ];
    let started = tokio::time::Instant::now();
    let (result, reporter, _work) = execute(host_job(steps)).await;
    let text = reporter.lines().join("\n");
    assert!(started.elapsed() < Duration::from_secs(18), "{:?}", started.elapsed());
    assert_eq!(result, TaskResult::Failure, "{text}");
    assert_eq!(statuses(&reporter), [StepStatus::Failure, StepStatus::Skipped, StepStatus::Success, StepStatus::Skipped], "{text}");
    assert!(text.contains("exceeded its timeout of 0.02 minutes") && text.contains("always ran"), "{text}");
    assert!(!reporter.lines().iter().any(|line| line == "after"), "the step was stopped: {text}");
}

#[tokio::test]
async fn steps_after_a_cancellation_share_a_budget() {
    let work = tempfile::tempdir().unwrap();
    let spec = host_job(vec![step("echo")]);
    let reporter = Arc::new(Reporter::new(spec.task_id, &work.path().join("task"), 1).unwrap());
    let mut job = Job::prepare(&loaded(work.path()), spec, reporter).unwrap();
    let signal = job.stop_signal(Some(Duration::from_secs(3600)));
    assert!(
        signal.interrupt.is_some() && signal.deadline.is_some_and(|(_, why)| why == StopReason::StepTimeout(Duration::from_secs(3600)))
    );

    job.interrupted_at = Some(Instant::now().checked_sub(CANCEL_BUDGET.checked_sub(Duration::from_secs(1)).unwrap()).unwrap());
    let short = job.stop_signal(Some(Duration::from_millis(1)));
    assert_eq!(short.deadline.map(|(_, why)| why), Some(StopReason::StepTimeout(Duration::from_millis(1))));
    let signal = job.stop_signal(Some(Duration::from_secs(3600)));
    assert!(signal.interrupt.is_none(), "a cancellation does not stop the steps it allows");
    assert_eq!(signal.deadline.map(|(_, why)| why), Some(StopReason::Budget));
    tokio::time::timeout(Duration::from_secs(5), signal.wait()).await.unwrap();
    assert_eq!(signal.reason(), StopReason::Budget);
}

fn cache_step(with: &[(&str, &str)]) -> StepTemplate {
    let mut template = step("");
    template.id = Some("cache".into());
    template.action = StepAction::Uses(rbit_workflow::spec::Builtin {
        uses: "actions/cache@v4".into(),
        kind: rbit_workflow::spec::BuiltinKind::Cache,
        version: 1,
        with: with.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
    });
    template
}

#[tokio::test]
async fn a_cache_saved_after_one_job_is_restored_in_the_next() {
    let work = tempfile::tempdir().unwrap();
    let executor = JobExecutor::new(Arc::new(loaded(work.path())));
    let run = |task: i64, steps: Vec<StepTemplate>| {
        let executor = executor.clone();
        let work = work.path().to_path_buf();
        async move {
            let mut spec = host_job(steps);
            spec.task_id = rbit_workflow::protocol::TaskId(task);
            spec.template.outputs = [("hit".to_owned(), "${{ steps.cache.outputs.cache-hit }}".to_owned())].into();
            let dir = crate::task::task_dir(&work, spec.task_id);
            let reporter = Arc::new(Reporter::new(spec.task_id, &dir, spec.template.steps.len()).unwrap());
            let result = executor.execute(spec, Arc::clone(&reporter)).await;
            (result, reporter)
        }
    };
    let key = [("key", "deps-${{ runner.os }}-1"), ("path", "cachedir")];
    let (result, reporter) = run(1, vec![cache_step(&key), step("mkdir cachedir\necho cached > cachedir/x")]).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(text.contains("No cache entry") && text.contains("Saved the cache entry `deps-"), "{text}");
    assert_eq!(reporter.state().outputs["hit"], "false");

    let (result, reporter) = run(2, vec![cache_step(&key), step("cat cachedir/x")]).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(text.contains("Restoring the cache entry") && reporter.lines().iter().any(|line| line.trim() == "cached"), "{text}");
    assert_eq!(reporter.state().outputs["hit"], "true");
    assert!(!text.contains("Saved the cache entry"), "an exact hit is not saved again");
    assert!(work.path().join("cache/1/refs/heads/main").is_dir(), "entries live under the repository and scope");

    // An entry that cannot be restored is a miss, not a failed job.
    for entry in std::fs::read_dir(work.path().join("cache/1/refs/heads/main")).unwrap() {
        let path = entry.unwrap().path();
        if path.to_string_lossy().ends_with(".tar.gz") {
            std::fs::write(&path, b"not an archive").unwrap();
        }
    }
    let (result, reporter) = run(3, vec![cache_step(&key), step("mkdir cachedir\necho rebuilt > cachedir/x")]).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(text.contains("##[warning]cannot restore the cache entry `deps-"), "{text}");
    assert_eq!(reporter.state().outputs["hit"], "false");
    assert!(text.contains("Saved the cache entry `deps-"), "the job's save replaces it: {text}");
    let (result, reporter) = run(4, vec![cache_step(&key), step("cat cachedir/x")]).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(reporter.lines().iter().any(|line| line.trim() == "rebuilt"), "{text}");
    assert_eq!(reporter.state().outputs["hit"], "true");
}

// Actions: third-party actions.
/// What an action's own `action.yml` evaluates (input defaults, composite steps) never sees
/// the workflow's secrets: such an expression fails, as on GitHub. The workflow passes one
/// with `with:` if it means to. Composite steps get `GITHUB_ACTION_PATH`.
#[tokio::test]
async fn actions_never_see_the_workflows_secrets_unless_given() {
    if find_program("sh").is_none() {
        return;
    }
    let sh = |script: &str| with(step(script), |step| step.shell = Some("sh".into()));
    // `${{` is built from `$D`, so this step's own template leaves the actions' expressions alone.
    let setup = concat!(
        "D='$' && mkdir -p default direct given\n",
        "printf 'inputs:\\n  stolen:\\n    default: \"%s{{ secrets.MY_SECRET }}\"\\nruns:\\n  using: composite\\n  steps:\\n    - shell: sh\\n      run: echo \"x%s{{ inputs.stolen }}\"\\n' \"$D\" \"$D\" > default/action.yml\n",
        "printf 'runs:\\n  using: composite\\n  steps:\\n    - shell: sh\\n      run: echo \"x%s{{ secrets.MY_SECRET }}\"\\n' \"$D\" > direct/action.yml\n",
        "printf 'inputs:\\n  given: {}\\nruns:\\n  using: composite\\n  steps:\\n    - shell: sh\\n      run: echo \"given=[%s{{ inputs.given }}] path-set=[%s{GITHUB_ACTION_PATH:+yes}]\"\\n' \"$D\" \"$D\" > given/action.yml\n",
    );
    let local = |path: &str, with: &[(&str, &str)]| {
        let rbit_workflow::builtins::Uses::Action(source) = rbit_workflow::builtins::resolve_uses(&format!("./{path}")).unwrap() else {
            panic!()
        };
        let mut uses = step("");
        uses.action = StepAction::Action(rbit_workflow::spec::ActionStep {
            uses: format!("./{path}"),
            source,
            with: with.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
        });
        uses.continue_on_error = Some("true".into());
        uses.id = Some(path.to_owned());
        uses
    };
    let steps = vec![sh(setup), local("default", &[]), local("direct", &[]), local("given", &[("given", "${{ secrets.MY_SECRET }}")])];
    let mut spec = host_job(steps);
    spec.template.outputs =
        [("outcomes".to_owned(), "${{ steps.default.outcome }}/${{ steps.direct.outcome }}/${{ steps.given.outcome }}".to_owned())].into();
    let (result, reporter, _work) = execute(spec).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert_eq!(reporter.state().outputs["outcomes"], "failure/failure/success", "{text}");
    assert_eq!(text.matches("the `secrets` context is not available here").count(), 2, "{text}");
    assert!(text.contains("given=[***] path-set=[yes]"), "only what the workflow passes reaches the action: {text}");
    assert!(!text.contains(SECRET), "{text}");
}

// Actions: settings API.
/// A job whose repository allows only its own actions refuses another's before fetching
/// it, and still runs the workspace's own.
#[tokio::test]
async fn the_repositorys_actions_policy_refuses_other_actions() {
    if find_program("sh").is_none() {
        return;
    }
    let action = |uses: &str| {
        let rbit_workflow::builtins::Uses::Action(source) = rbit_workflow::builtins::resolve_uses(uses).unwrap() else { panic!() };
        let mut step = step("");
        step.action =
            StepAction::Action(rbit_workflow::spec::ActionStep { uses: uses.to_owned(), source, with: std::collections::BTreeMap::new() });
        step.continue_on_error = Some("true".into());
        step
    };
    let setup = with(
        step(
            "mkdir -p own && printf 'runs:\n  using: composite\n  steps:\n    - shell: sh\n      run: echo own action ran\n' > own/action.yml",
        ),
        |step| {
            step.shell = Some("sh".into());
        },
    );
    let mut spec = host_job(vec![setup, action("evil/tool@v1"), action("./own")]);
    spec.actions_policy = rbit_workflow::spec::ActionsPolicy { rules: vec![rbit_workflow::spec::ActionsRule::LocalOnly] };
    let (result, reporter, _work) = execute(spec).await;
    let text = reporter.lines().join("\n");
    assert_eq!(result, TaskResult::Success, "{text}");
    assert!(
        text.contains("`evil/tool@v1` is not allowed by the repository's Actions permissions: only the repository's own actions"),
        "{text}"
    );
    assert!(text.contains("own action ran"), "{text}");
}
