//! Test support: an in-process stand-in for the server's runner endpoint, job
//! specifications to hand out, and an executor made from a closure.
//!
//! The fake speaks protocol v1 as §12.6 describes it — headers and credential checked,
//! polls held open and idempotent per key, logs stored from the server's own line count
//! with `409 {next_line}` on a gap, cancellation in every state answer — and can be told
//! to fail the next calls of an endpoint, to forget a task or to cancel it.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path as UrlPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use rbit_workflow::{
    protocol::{
        DeclareRequest, DeclareResponse, ErrorBody, ErrorCode, Intervals, LogsQuery, LogsResponse, PollRequest, PollResponse, RunnerId,
        TaskId, TaskResult, TaskStateRequest, TaskStateResponse,
    },
    secret::Secret,
    spec::{
        CacheScopes, CheckoutSource, Contexts, Env, JOB_SPEC_VERSION, JOB_TEMPLATE_VERSION, JobSpec, JobTemplate, RunDefaults, StepAction,
        StepTemplate, Strategy,
    },
};
use reqwest::Url;
use serde_json::Map;
use tokio::sync::Notify;

use crate::{
    client::Client,
    config::{self, Loaded},
    task::{Executor, Reporter},
};

pub const CREDENTIAL: &str = "rbrn_DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD";
pub const TOKEN: &str = "rbat_EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE";

/// A valid job of `steps` `run:` steps.
pub fn job_spec(task_id: i64, steps: usize) -> JobSpec {
    JobSpec {
        version: JOB_SPEC_VERSION,
        task_id: TaskId(task_id),
        template: JobTemplate {
            version: JOB_TEMPLATE_VERSION,
            job_id: "build".into(),
            name: "build".into(),
            needs: Vec::new(),
            condition: None,
            runs_on: vec!["ubuntu-latest".into()],
            matrix: Map::new(),
            strategy: Strategy::SINGLE,
            permissions: None,
            concurrency: None,
            timeout_minutes: 360,
            continue_on_error: false,
            workflow_env: Env::new(),
            env: Env::new(),
            defaults: RunDefaults::default(),
            outputs: BTreeMap::new(),
            container: None,
            services: Vec::new(),
            steps: (0..steps)
                .map(|index| StepTemplate {
                    id: None,
                    name: None,
                    condition: None,
                    action: StepAction::Run(format!("echo step {index}")),
                    shell: None,
                    working_directory: None,
                    env: Env::new(),
                    continue_on_error: None,
                    timeout_minutes: None,
                })
                .collect(),
            environment: None,
            environment_url: None,
            calls: Vec::new(),
            need_names: std::collections::BTreeMap::new(),
        },
        contexts: Contexts { github: Map::new(), inputs: Map::new(), vars: BTreeMap::new(), needs: BTreeMap::new() },
        secrets: BTreeMap::new(),
        token: Secret::new(TOKEN),
        checkout: CheckoutSource {
            url: "https://git.example.com/alice/app".into(),
            repository: "alice/app".into(),
            sha: "0123456789abcdef0123456789abcdef01234567".into(),
            reference: "refs/heads/main".into(),
        },
        cache: CacheScopes { repository_id: 1, read: vec!["refs/heads/main".into()], write: "refs/heads/main".into() },
        actions_policy: rbit_workflow::spec::ActionsPolicy::default(),
    }
}

type BoxedRun = Box<dyn Fn(JobSpec, Arc<Reporter>) -> std::pin::Pin<Box<dyn Future<Output = TaskResult> + Send>> + Send + Sync>;

/// An executor that runs a closure.
pub struct FnExecutor {
    run: BoxedRun,
}

impl std::fmt::Debug for FnExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FnExecutor")
    }
}

impl FnExecutor {
    pub fn new<F, Fut>(run: F) -> Self
    where
        F: Fn(JobSpec, Arc<Reporter>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = TaskResult> + Send + 'static,
    {
        Self { run: Box::new(move |spec, reporter| Box::pin(run(spec, reporter))) }
    }
}

impl Executor for FnExecutor {
    fn execute(&self, spec: JobSpec, reporter: Arc<Reporter>) -> impl Future<Output = TaskResult> + Send {
        (self.run)(spec, reporter)
    }
}

/// One poll the fake received.
#[derive(Debug, Clone)]
pub struct Poll {
    pub key: Option<String>,
    pub free_slots: u32,
}

#[derive(Debug, Default)]
struct Fake {
    queue: VecDeque<JobSpec>,
    /// Tasks already handed out, by idempotency key.
    claimed: HashMap<String, JobSpec>,
    polls: Vec<Poll>,
    declares: Vec<DeclareRequest>,
    states: BTreeMap<TaskId, Vec<TaskStateRequest>>,
    logs: BTreeMap<TaskId, Vec<String>>,
    log_calls: BTreeMap<TaskId, Vec<(u64, u16)>>,
    cancelled: HashSet<TaskId>,
    forgotten: HashSet<TaskId>,
    /// Endpoint → (status, calls left, Retry-After seconds).
    failures: HashMap<&'static str, (u16, u32, Option<u64>)>,
    unregistered: u32,
}

#[derive(Debug, Default)]
struct Shared {
    fake: Mutex<Fake>,
    task_added: Notify,
}

/// The fake server, listening on a loopback port until the test ends.
#[derive(Debug, Clone)]
pub struct FakeServer {
    url: Url,
    shared: Arc<Shared>,
}

impl FakeServer {
    pub async fn start() -> Self {
        let shared = Arc::<Shared>::default();
        let routes = Router::new()
            .route("/declare", post(declare))
            .route("/poll", post(poll))
            .route("/tasks/{id}/state", post(state))
            .route("/tasks/{id}/logs", post(logs))
            .route("/unregister", post(unregister))
            .with_state(Arc::clone(&shared));
        let app = Router::new().nest(rbit_workflow::protocol::BASE_PATH, routes);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a loopback port");
        let address = listener.local_addr().expect("the port");
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url: Url::parse(&format!("http://{address}")).expect("a URL"), shared }
    }

    pub fn client(&self) -> Client {
        Client::new(&self.url, Some(Secret::new(CREDENTIAL))).expect("a client")
    }

    /// A configuration for this server with capacity 2, its work directory at `work`.
    pub fn loaded(&self, work: &Path) -> Loaded {
        let mut config = config::tests::config();
        config.url = self.url.as_str().trim_end_matches('/').to_owned();
        config.insecure_http = true;
        config.labels.truncate(1);
        Loaded { config, work_dir: work.to_path_buf(), credential_path: work.join(config::CREDENTIAL_FILE) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Fake> {
        self.shared.fake.lock().expect("the fake's lock")
    }

    pub fn push_task(&self, spec: JobSpec) {
        self.lock().queue.push_back(spec);
        self.shared.task_added.notify_waiters();
    }

    /// The next `times` calls to `endpoint` (`declare`, `poll`, `state`, `logs`,
    /// `unregister`) answer `status`.
    pub fn fail_next(&self, endpoint: &'static str, status: u16, times: u32) {
        self.lock().failures.insert(endpoint, (status, times, None));
    }

    /// As [`Self::fail_next`], with a `Retry-After` of `seconds`.
    pub fn fail_next_after(&self, endpoint: &'static str, status: u16, times: u32, seconds: u64) {
        self.lock().failures.insert(endpoint, (status, times, Some(seconds)));
    }

    pub fn cancel(&self, task: TaskId) {
        self.lock().cancelled.insert(task);
    }

    /// From now on the task's calls answer `404`.
    pub fn forget(&self, task: TaskId) {
        self.lock().forgotten.insert(task);
    }

    /// Drops the task's log lines from `keep` on, as if the server had lost them.
    pub fn truncate_log(&self, task: TaskId, keep: usize) {
        self.lock().logs.entry(task).or_default().truncate(keep);
    }

    pub fn polls(&self) -> Vec<Poll> {
        self.lock().polls.clone()
    }

    pub fn declares(&self) -> Vec<DeclareRequest> {
        self.lock().declares.clone()
    }

    pub fn states(&self, task: TaskId) -> Vec<TaskStateRequest> {
        self.lock().states.get(&task).cloned().unwrap_or_default()
    }

    pub fn final_state(&self, task: TaskId) -> Option<TaskStateRequest> {
        self.states(task).into_iter().rev().find(TaskStateRequest::is_final)
    }

    pub fn log(&self, task: TaskId) -> Vec<String> {
        self.lock().logs.get(&task).cloned().unwrap_or_default()
    }

    /// `(from, status)` of every log append.
    pub fn log_calls(&self, task: TaskId) -> Vec<(u64, u16)> {
        self.lock().log_calls.get(&task).cloned().unwrap_or_default()
    }

    pub fn unregistered(&self) -> u32 {
        self.lock().unregistered
    }

    /// Waits until `condition` holds, for at most 10 seconds.
    pub async fn wait_for(&self, condition: impl Fn(&Self) -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !condition(self) {
            assert!(tokio::time::Instant::now() < deadline, "the condition did not hold within 10 seconds");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

const INTERVALS: Intervals = Intervals { long_poll_seconds: 1, heartbeat_seconds: 1 };

fn error(status: u16, retry_after: Option<u64>) -> Response {
    let code = match status {
        400 => ErrorCode::BadRequest,
        401 => ErrorCode::Unauthenticated,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        426 => return (StatusCode::UPGRADE_REQUIRED, Json(ErrorBody::unsupported_protocol())).into_response(),
        429 => ErrorCode::Throttled,
        _ => ErrorCode::Unavailable,
    };
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
    let mut response = (status, Json(ErrorBody::new(code, "scripted failure"))).into_response();
    if let Some(seconds) = retry_after {
        response.headers_mut().insert(header::RETRY_AFTER, seconds.into());
    }
    response
}

/// The answer refusing a call: a failed protocol check or a scripted failure.
fn admit(shared: &Shared, headers: &HeaderMap, endpoint: &'static str) -> Option<Response> {
    if headers.get("rbit-runner-protocol").and_then(|value| value.to_str().ok()) != Some("1") {
        return Some(error(426, None));
    }
    let agent = headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();
    assert!(agent.starts_with("rbit-runner/"), "User-Agent {agent:?}");
    if headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) != Some(&format!("Bearer {CREDENTIAL}")) {
        return Some(error(401, None));
    }
    let mut fake = shared.fake.lock().expect("the fake's lock");
    if let Some((status, left, retry_after)) = fake.failures.get_mut(endpoint)
        && *left > 0
    {
        *left -= 1;
        return Some(error(*status, *retry_after));
    }
    None
}

async fn declare(State(shared): State<Arc<Shared>>, headers: HeaderMap, Json(request): Json<DeclareRequest>) -> Response {
    if let Some(response) = admit(&shared, &headers, "declare") {
        return response;
    }
    request.validate().expect("a valid declaration");
    shared.fake.lock().expect("the fake's lock").declares.push(request);
    Json(DeclareResponse { runner_id: RunnerId(1), intervals: INTERVALS }).into_response()
}

async fn poll(State(shared): State<Arc<Shared>>, headers: HeaderMap, Json(request): Json<PollRequest>) -> Response {
    let key = headers.get("idempotency-key").and_then(|value| value.to_str().ok()).map(str::to_owned);
    shared.fake.lock().expect("the fake's lock").polls.push(Poll { key: key.clone(), free_slots: request.free_slots });
    if let Some(response) = admit(&shared, &headers, "poll") {
        return response;
    }
    request.validate().expect("a valid poll");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(INTERVALS.long_poll_seconds.into());
    loop {
        let added = shared.task_added.notified();
        tokio::pin!(added);
        // Registered before the queue is looked at, so a task pushed in between wakes it.
        added.as_mut().enable();
        {
            let mut fake = shared.fake.lock().expect("the fake's lock");
            let again = key.as_ref().and_then(|key| fake.claimed.get(key).cloned());
            if let Some(task) = again.or_else(|| fake.queue.pop_front()) {
                if let Some(key) = &key {
                    fake.claimed.insert(key.clone(), task.clone());
                }
                return Json(PollResponse { task }).into_response();
            }
        }
        if tokio::time::timeout_at(deadline, added).await.is_err() {
            return StatusCode::NO_CONTENT.into_response();
        }
    }
}

async fn state(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<i64>,
    headers: HeaderMap,
    Json(request): Json<TaskStateRequest>,
) -> Response {
    if let Some(response) = admit(&shared, &headers, "state") {
        return response;
    }
    let task = TaskId(id);
    let mut fake = shared.fake.lock().expect("the fake's lock");
    if fake.forgotten.contains(&task) {
        return error(404, None);
    }
    request.validate().expect("a valid state");
    fake.states.entry(task).or_default().push(request);
    Json(TaskStateResponse { cancel: fake.cancelled.contains(&task) }).into_response()
}

async fn logs(
    State(shared): State<Arc<Shared>>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<LogsQuery>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Some(response) = admit(&shared, &headers, "logs") {
        shared
            .fake
            .lock()
            .expect("the fake's lock")
            .log_calls
            .entry(TaskId(id))
            .or_default()
            .push((query.from, response.status().as_u16()));
        return response;
    }
    let task = TaskId(id);
    let mut fake = shared.fake.lock().expect("the fake's lock");
    if fake.forgotten.contains(&task) {
        return error(404, None);
    }
    assert!(body.ends_with('\n'), "log bodies are whole lines");
    let lines: Vec<String> = body.strip_suffix('\n').unwrap_or(&body).split('\n').map(str::to_owned).collect();
    let stored = fake.logs.entry(task).or_default();
    let count = stored.len() as u64;
    if query.from > count {
        fake.log_calls.entry(task).or_default().push((query.from, 409));
        return (StatusCode::CONFLICT, Json(ErrorBody::log_gap(count))).into_response();
    }
    let skip = usize::try_from(count - query.from).expect("a small log");
    stored.extend(lines.into_iter().skip(skip));
    let next_line = stored.len() as u64;
    fake.log_calls.entry(task).or_default().push((query.from, 200));
    Json(LogsResponse { next_line }).into_response()
}

async fn unregister(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if let Some(response) = admit(&shared, &headers, "unregister") {
        return response;
    }
    shared.fake.lock().expect("the fake's lock").unregistered += 1;
    StatusCode::NO_CONTENT.into_response()
}
