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
    execute_with(spec, |_| {}).await
}

async fn execute_with(spec: JobSpec, during: impl FnOnce(Arc<Reporter>)) -> (TaskResult, Arc<Reporter>, tempfile::TempDir) {
    let work = tempfile::tempdir().unwrap();
    let reporter =
        Arc::new(Reporter::new(spec.task_id, &crate::task::task_dir(work.path(), spec.task_id), spec.template.steps.len()).unwrap());
    for value in spec.secrets.values().chain([&spec.token]) {
        reporter.add_mask(value.expose());
    }
    during(Arc::clone(&reporter));
    let executor = JobExecutor::new(Arc::new(loaded(work.path())));
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
        sh("echo started; sleep 8"),
        sh("echo not run"),
        with(sh("echo cleanup ran"), |step| step.condition = Some("always()".into())),
    ];
    let started = tokio::time::Instant::now();
    let (result, reporter, _work) = execute_with(host_job(steps), |reporter| {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            reporter.cancellation().cancel();
        });
    })
    .await;
    assert!(started.elapsed() < Duration::from_secs(20));
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
