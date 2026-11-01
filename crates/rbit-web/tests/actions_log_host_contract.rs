//! Actual log transport/state transitions underpin the pre/post content comparison.
use super::native_actions_summary_host_contract::{
    Document, assert_host_document, measured_summary as measured_document, sql_multiset_digest,
};
use super::*;
use crate::actions_runner_tasks::{logs, payload, poll, seed, spec, state, template};
use rbit_core::{
    Timestamp,
    actions::{Job, Run, TaskId},
};

struct Case {
    fixture: Fixture,
    credential: String,
    run: Run,
    job: Job,
    task: i64,
    cookie: String,
    initial_log: String,
}
impl Case {
    async fn new() -> Self {
        let fixture = Fixture::with(|config| config.actions.max_log_bytes = 64 * 1024).await;
        unread_notification(&fixture).await;
        let credential = fixture.runner(&["linux"]).await;
        let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
        let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
        let task = spec(&poll(&fixture, &credential, Some("log-host-contract")).await.1).task_id.0;
        let body = serde_json::json!({"steps":[{"index":0,"status":"running","started_at":100,"stopped_at":null,"log_index":0,"log_length":0}],"outputs":{},"result":null});
        assert_eq!(state(&fixture, &credential, task, &body).await.0, StatusCode::OK);
        let initial_log = format!(
            "COMPLETE_EARLY_RAW_LOG\n{}##[error]\u{1b}[31m<script>log_untrusted()</script>\u{1b}[0m\n",
            "ordinary line\n".repeat(270)
        );
        assert_eq!(logs(&fixture, &credential, task, 0, &initial_log).await.0, StatusCode::OK);
        let cookie = format!("{}; rbit-theme=base24-berlin", cookie(&fixture).await);
        Self { fixture, credential, run, job, task, cookie, initial_log }
    }
    fn job_path(&self) -> String {
        format!("/alice/rocket/-/plugins/rbit.actions/job?run={}&job={}", self.run.number, self.job.id.0)
    }
    fn step_path(&self) -> String {
        format!("/alice/rocket/-/plugins/rbit.actions/logs?run={}&job={}&step=0", self.run.number, self.job.id.0)
    }
    fn raw_path(&self) -> String {
        format!("/alice/rocket/actions/runs/{}/jobs/{}/logs/raw", self.run.number, self.job.id.0)
    }
    async fn truncate(&self) -> u64 {
        let before = self.fixture.database.actions_runs().task(TaskId(self.task)).await.unwrap().unwrap();
        let extra = format!("{}\n", "x".repeat(512)).repeat(170);
        assert_eq!(
            logs(&self.fixture, &self.credential, self.task, u64::try_from(before.log_lines).unwrap(), &extra).await.0,
            StatusCode::OK
        );
        let after = self.fixture.database.actions_runs().task(TaskId(self.task)).await.unwrap().unwrap();
        assert!(after.log_truncated, "real configured log quota must have been reached");
        u64::try_from(after.log_lines).unwrap()
    }
    async fn finish(&self, lines: u64) {
        let body = serde_json::json!({"steps":[{"index":0,"status":"success","started_at":100,"stopped_at":101,"log_index":0,"log_length":lines}],"outputs":{},"result":"success"});
        assert_eq!(state(&self.fixture, &self.credential, self.task, &body).await.0, StatusCode::OK);
    }
}
fn escaped_href(path: &str) -> String {
    format!("href=\"{}\"", Escaped { value: path }.render().unwrap())
}
fn assert_polling(document: &Document, id: &str, path: &str, active: bool) {
    let tag = document
        .html
        .split("<section ")
        .filter_map(|section| section.split('>').next())
        .find(|tag| tag.contains(&format!("id=\"{id}\"")))
        .unwrap();
    if active {
        assert!(tag.contains(&format!("hx-get=\"{}\"", Escaped { value: path }.render().unwrap())));
        assert!(tag.contains("hx-trigger=\"every 3s\"") && tag.contains(&format!("hx-select=\"#{id}\"")));
        assert!(tag.contains("hx-target=\"this\"") && tag.contains("hx-swap=\"outerHTML\""));
    } else {
        assert!(!tag.contains("hx-get="), "terminal/expired step must not retain live polling");
    }
}
async fn measure_phase(case: &Case, phase: &str, active: bool) -> (Document, Document) {
    for path in [case.job_path(), case.step_path()] {
        assert_eq!(request(&case.fixture, &case.cookie, Method::GET, &path, String::new()).await.0, StatusCode::OK);
    }
    let job = measured_document(&case.fixture, &case.cookie, &case.job_path()).await;
    let step = measured_document(&case.fixture, &case.cookie, &case.step_path()).await;
    let site = case.fixture.services.instance.current().site_name;
    assert_host_document(
        &job,
        &format!("{} · {} #{} · alice/rocket · {site}", case.job.name, case.run.workflow_name, case.run.number),
        &case.job.name,
    );
    assert_host_document(&step, &format!("Build · {} · alice/rocket · {site}", case.job.name), "Build");
    assert_polling(&job, "job", &case.job_path(), active);
    assert_polling(&step, "step", &case.step_path(), active);
    for document in [&job, &step] {
        assert!(document.html.contains(&escaped_href(&case.raw_path())));
        assert!(!document.html.contains("<script>log_untrusted()"));
    }
    assert!(job.html.contains("data-step=\"0\"") && job.html.contains("data-step=\"1\""));
    let generation = case.fixture.services.actions_status().await.unwrap().generation.unwrap().get();
    let tag = job.html.split("<section ").filter_map(|section| section.split('>').next()).find(|tag| tag.contains("id=\"job\"")).unwrap();
    assert!(tag.contains("data-plugin-owner=\"rbit.actions\"") && tag.contains(&format!("data-plugin-generation=\"{generation}\"")));
    assert!(tag.contains("data-plugin-enhancement=\"actions.log\""));
    assert!(
        tag.contains(&format!("data-plugin-module=\"{}\"", Escaped { value: &rbit_web::assets::url("actions-log.js") }.render().unwrap()))
    );
    for (route, document) in [("job", &job), ("step", &step)] {
        let mut sql = document.queries.clone();
        sql.sort();
        eprintln!("log host SQL multiset: phase={phase} route={route} count={} sha256={}", sql.len(), sql_multiset_digest(&sql));
    }
    // Different route responsibilities legitimately have different SQL work.
    // Root persists each phase/route independently before and after extraction.
    (job, step)
}
#[tokio::test]
async fn live_truncated_final_and_expired_logs_preserve_host_work_and_document_contracts() {
    let case = Case::new().await;
    let (job, step) = measure_phase(&case, "live", true).await;
    for document in [&job, &step] {
        assert!(document.html.contains("<span class=\"text-danger\">Error: &lt;script&gt;log_untrusted()&lt;/script&gt;</span>"));
        assert!(!document.html.contains('\u{1b}'));
        assert!(!document.html.contains("COMPLETE_EARLY_RAW_LOG"));
        assert!(document.html.contains("Earlier lines are left out."));
    }
    assert!(job.html.contains("This step has not started."));
    assert!(job.html.contains(&escaped_href(&case.step_path())));
    assert!(!step.html.contains("Show more of this step"));
    let (status, headers, raw) = request(&case.fixture, &case.cookie, Method::GET, &case.raw_path(), String::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, case.initial_log);
    assert_eq!(headers[header::CONTENT_SECURITY_POLICY], "sandbox; default-src 'none'");
    let lines = case.truncate().await;
    let (job, step) = measure_phase(&case, "truncated-live", true).await;
    for document in [&job, &step] {
        assert!(document.html.contains("later lines were dropped."));
    }
    case.finish(lines).await;
    let (job, step) = measure_phase(&case, "final", false).await;
    for document in [&job, &step] {
        assert!(document.html.contains("later lines were dropped."));
    }
    // Supported retention metadata transition while files deliberately remain:
    // this checks authoritative redaction, not a filesystem sweeper proof.
    case.fixture.database.actions_runs().expire_logs(case.run.id, Timestamp::now()).await.unwrap();
    let (job, step) = measure_phase(&case, "expired", false).await;
    for document in [&job, &step] {
        assert!(document.html.contains("This log was removed by the log retention."));
        assert!(!document.html.contains("log_untrusted()"));
        assert!(!document.html.contains("class=\"code-line\""));
    }
    assert_eq!(request(&case.fixture, &case.cookie, Method::GET, &case.raw_path(), String::new()).await.0, StatusCode::NOT_FOUND);
    case.fixture.services.shutdown().await;
}
