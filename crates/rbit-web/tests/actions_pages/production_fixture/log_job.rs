//! Synthetic runner-backed logs through existing services, never host data or commands.
use super::Fixture;
use rbit_core::{Timestamp, actions::TaskId};
use rbit_services::actions::runners::{CachedRunner, ManagedScope, RegistrationOptions};
use rbit_storage::sqlite::actions_runs::{NewJob, NewRun, RunStart};
use rbit_workflow::protocol::{PollRequest, RegisterRequest, TaskStateRequest};
use std::fmt::Write;
use std::sync::Arc;

pub(super) struct LogJob {
    runner: Arc<CachedRunner>,
    task: TaskId,
    started: i64,
}

fn output() -> String {
    let mut text = String::new();
    for number in 1..=1205 {
        if number == 500 {
            text.push_str("line 0500: <synthetic> & output\n");
        } else {
            writeln!(text, "line {number:04}").unwrap();
        }
    }
    text.push_str("waiting for finish\n");
    text
}

async fn seed_runs(fixture: &Fixture) {
    use crate::actions_runner_tasks::{payload, template};
    let mut job = template("visual-log", &["visual-log"], &[]);
    job.name = "Build / stable <synthetic> & matrix".into();
    job.steps[0].name = Some("Compile <synthetic> & dependencies".into());
    job.steps[1].name = Some("Wait for synthetic completion".into());
    let (_, payload) = payload(&job);
    let labels = vec!["visual-log".to_owned()];
    let jobs = [NewJob {
        key: "visual-log",
        name: &job.name,
        matrix: None,
        needs: &[],
        runs_on: &labels,
        permissions: None,
        payload: &payload,
        condition: false,
        max_parallel: None,
        group: None,
        environment: None,
    }];
    for number in 2..=26 {
        let title = format!("Synthetic change {number:02}");
        let run = fixture
            .database
            .actions_runs()
            .create_run(&NewRun {
                repo: fixture.repo_id().await,
                workflow_path: ".rbit/workflows/ci.yml",
                workflow_name: "Continuous integration",
                title: &title,
                event: if number == 2 { rbit_core::actions::RunEvent::Push } else { rbit_core::actions::RunEvent::PullRequest },
                reference: if number % 2 == 0 { "refs/heads/main" } else { "refs/heads/feature" },
                sha: "0123456789abcdef0123456789abcdef01234567",
                actor: Some(fixture.alice.id()),
                trigger_actor: Some(fixture.alice.id()),
                pull_number: None,
                is_fork: number != 2,
                trusted: number == 2,
                concurrency_group: None,
                cancel_in_progress: false,
                event_payload: "{}",
                dispatch_inputs: None,
                start: if number == 2 { RunStart::Runnable } else { RunStart::AwaitingApproval },
                jobs: &jobs,
                error: None,
                now: Timestamp::now(),
            })
            .await
            .unwrap();
        assert_eq!(run.number, number, "FIXTURE_RUN_ID_REFUSED");
        if number == 2 {
            let selected = fixture.database.actions_runs().jobs(run.id).await.unwrap();
            assert_eq!(selected.len(), 1, "FIXTURE_JOB_ID_REFUSED");
            assert_eq!(selected[0].id.0, 2, "FIXTURE_JOB_ID_REFUSED");
        }
    }
}

impl LogJob {
    pub(super) async fn seed(fixture: &Fixture) -> Self {
        seed_runs(fixture).await;
        let actions = fixture.services.actions.as_ref().unwrap();
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
        let token = actions
            .create_registration_token(&fixture.alice, ManagedScope::Repository(&repo), RegistrationOptions::default())
            .await
            .unwrap();
        let request: RegisterRequest =
            serde_json::from_value(crate::actions_runner::register_body(token.secret.expose(), "synthetic-log-runner", &["visual-log"]))
                .unwrap();
        let registered = actions.register_runner(&request).await.unwrap();
        let runner = actions.authenticate_runner(registered.credential.expose()).await.unwrap().unwrap();
        actions.bump_tasks_version();
        let spec = actions.poll(&runner, &PollRequest { free_slots: 1 }, Some("synthetic-log")).await.unwrap().unwrap();
        let task = TaskId(spec.task_id.0);
        let stored = fixture.database.actions_runs().task(task).await.unwrap().unwrap();
        assert_eq!(stored.job_id.0, 2, "FIXTURE_JOB_CLAIM_REFUSED");
        let mut body = output();
        // Exercise the real server masking pass without exposing this runtime token.
        writeln!(body, "task token {}", spec.token.expose()).unwrap();
        drop(spec); // The runner/task credentials never leave this fixture process.
        let appended = actions.append_log(&runner, task, 0, body.as_bytes()).await.unwrap();
        assert_eq!(appended.next_line, 1207, "FIXTURE_LOG_LINES_REFUSED");
        let owned = Self { runner, task, started: stored.started_at.seconds() };
        owned.state(fixture, false).await;
        owned
    }

    async fn state(&self, fixture: &Fixture, finished: bool) {
        let request: TaskStateRequest = serde_json::from_value(serde_json::json!({
            "steps": [
                {"index": 0, "status": "failure", "started_at": self.started, "stopped_at": self.started,
                 "log_index": 0, "log_length": 1205},
                {"index": 1, "status": if finished { "success" } else { "running" }, "started_at": self.started,
                 "stopped_at": if finished { Some(Timestamp::now().seconds().max(self.started)) } else { None },
                 "log_index": 1205, "log_length": 2}
            ], "outputs": {}, "result": if finished { Some("failure") } else { None }
        }))
        .unwrap();
        fixture.services.actions.as_ref().unwrap().task_state(&self.runner, self.task, &request).await.unwrap();
    }

    pub(super) async fn finish(&self, fixture: &Fixture) {
        self.state(fixture, true).await;
        let task = fixture.database.actions_runs().task(self.task).await.unwrap().unwrap();
        assert_eq!(task.log_state, rbit_core::actions::LogState::Final, "FIXTURE_GZIP_UNPROVED");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_log_is_bounded_and_contains_known_text_boundaries() {
        let body = output();
        assert_eq!(body.lines().count(), 1206);
        assert!(body.starts_with("line 0001\n") && body.ends_with("line 1205\nwaiting for finish\n"));
        assert!(body.contains("line 0500: <synthetic> & output\n"));
        assert!(body.len() < 16 * 1024);
    }

    #[tokio::test]
    async fn runner_fixture_keeps_masked_live_and_finished_raw_logs_identical() {
        use rbit_core::actions::JobId;
        use rbit_services::actions::logs::{LogLimits, LogRange};
        use std::io::Read;
        let fixture = Fixture::new().await;
        super::super::linux::seed(&fixture).await;
        let job = LogJob::seed(&fixture).await;
        let actions = fixture.services.actions.as_ref().unwrap();
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
        let read = actions.job_log_detail(&repo, 2, JobId(2), LogLimits { lines: 1000, bytes: 256 * 1024 }).await.unwrap();
        assert_eq!(read.logs[0].as_ref().unwrap().lines.len(), 1000);
        assert_eq!(read.logs[0].as_ref().unwrap().first_line, 205);
        assert!(read.detail.steps.iter().all(|step| step.started_at == Some(Timestamp(job.started))));
        let live = actions.task_log(&repo, job.task, LogRange::From(0), 1300, 16384).await.unwrap();
        assert_eq!(live.lines.len(), 1207);
        assert_eq!(live.lines.last().unwrap(), "task token ***");
        job.finish(&fixture).await;
        let final_window = actions.task_log(&repo, job.task, LogRange::From(0), 1300, 16384).await.unwrap();
        assert_eq!(final_window.lines, live.lines);
        let raw = actions.raw_log(&repo, job.task).await.unwrap().unwrap();
        assert!(raw.gzip);
        let mut compressed = Vec::new();
        let mut file = tokio::io::AsyncReadExt::take(raw.file, 16384);
        tokio::io::AsyncReadExt::read_to_end(&mut file, &mut compressed).await.unwrap();
        drop(file);
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(compressed.as_slice()).take(16384).read_to_string(&mut decoded).unwrap();
        assert_eq!(decoded, format!("{}task token ***\n", output()));
    }
}
