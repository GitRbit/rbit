//! rbit Actions pages: a repository's runs, one run with its jobs, and one job with its
//! steps and their logs.
//!
//! Colour marks state only (`text-success`, `text-danger`, `text-warning`); everything else
//! stays monochrome. Every page works without JavaScript: the run page refreshes its job
//! list with htmx while the run is not finished, and a reload does the same.
//!
//! Workstream U (interface and secrets) owns this file and `templates/actions/**`.

use std::collections::{BTreeMap, HashMap};

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Account, AccountId, Timestamp,
    actions::{Job, Run, RunEvent, Status, Step},
};
use rbit_services::actions::{logs::LogWindow, statuses::duration};

use crate::{
    render::escape_html,
    views::{
        blocks::{Block, blocks, column_class, markup_columns},
        layout::Page,
        repo::{CodeLine, RepoHeader},
    },
};

/// How a state is shown: the key picks the icon in `actions/state.html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateLook {
    /// `success`, `failure`, `cancelled`, `skipped`, `queued`, `running`, `waiting`,
    /// `approval` or `error`.
    pub key: &'static str,
    pub label: &'static str,
    pub tone: &'static str,
}

impl StateLook {
    pub const fn of(status: Status) -> Self {
        let (key, label, tone) = match status {
            Status::Success => ("success", "Success", "text-success"),
            Status::Failure => ("failure", "Failure", "text-danger"),
            Status::Cancelled => ("cancelled", "Cancelled", "text-ink-muted"),
            Status::Skipped => ("skipped", "Skipped", "text-ink-faint"),
            Status::Waiting => ("queued", "Queued", "text-warning"),
            Status::Running => ("running", "In progress", "text-warning"),
            Status::Blocked => ("waiting", "Waiting", "text-ink-muted"),
            Status::AwaitingApproval => ("approval", "Awaiting approval", "text-warning"),
        };
        Self { key, label, tone }
    }

    /// A run whose workflow failed to parse or plan.
    pub const WORKFLOW_ERROR: Self = Self { key: "error", label: "Workflow error", tone: "text-danger" };

    fn of_run(run: &Run) -> Self {
        if run.is_workflow_error() { Self::WORKFLOW_ERROR } else { Self::of(run.status) }
    }
}

/// `main` for `refs/heads/main`, `#3` for a pull request ref, the tag name for a tag.
pub fn short_ref(reference: &str) -> String {
    if let Some(branch) = reference.strip_prefix("refs/heads/") {
        branch.to_owned()
    } else if let Some(pull) = reference.strip_prefix("refs/pull/").and_then(|rest| rest.split('/').next()) {
        format!("#{pull}")
    } else {
        reference.strip_prefix("refs/tags/").unwrap_or(reference).to_owned()
    }
}

/// The event as people say it.
pub const fn event_label(event: RunEvent) -> &'static str {
    match event {
        RunEvent::Push => "push",
        RunEvent::PullRequest => "pull request",
        RunEvent::WorkflowDispatch => "manual",
        RunEvent::Schedule => "schedule",
    }
}

/// How long something took: from its start to its end, or to now while it runs.
fn elapsed(started: Option<Timestamp>, finished: Option<Timestamp>) -> Option<String> {
    let started = started?;
    let end = finished.unwrap_or_else(Timestamp::now);
    Some(duration(end.seconds() - started.seconds()))
}

fn name_of(people: &BTreeMap<AccountId, Account>, id: Option<AccountId>) -> Option<String> {
    id.and_then(|id| people.get(&id)).map(|account| account.name.to_string())
}

/// Where the Actions pages of one repository are.
#[derive(Debug, Clone)]
pub struct ActionsUrls {
    base: String,
}

impl ActionsUrls {
    /// For the repository at `home` (`/{owner}/{repo}`).
    pub fn new(home: &str) -> Self {
        Self { base: format!("{home}/actions") }
    }

    pub fn runs(&self) -> String {
        self.base.clone()
    }

    /// The runs of one workflow, by its file name.
    pub fn workflow(&self, path: &str) -> String {
        format!("{}/workflows/{}", self.base, workflow_file(path))
    }

    pub fn run(&self, number: i64) -> String {
        format!("{}/runs/{number}", self.base)
    }

    pub fn run_action(&self, number: i64, action: &str) -> String {
        format!("{}/runs/{number}/{action}", self.base)
    }

    pub fn job(&self, number: i64, job: i64) -> String {
        format!("{}/runs/{number}/jobs/{job}", self.base)
    }
}

/// `ci.yml` of `.rbit/workflows/ci.yml`: workflows live in one directory, so the file name
/// names one.
pub fn workflow_file(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// How long `job` ran, or has been running.
pub fn job_duration(job: &Job) -> Option<String> {
    elapsed(job.started_at, job.finished_at)
}

/// One row of the runs list.
#[derive(Debug, Clone)]
pub struct RunRow {
    pub url: String,
    pub look: StateLook,
    pub title: String,
    pub number: i64,
    pub workflow: String,
    pub event: &'static str,
    pub reference: String,
    pub actor: Option<String>,
    pub when: String,
    pub when_iso: String,
    pub duration: Option<String>,
    /// Why the workflow has no jobs, for workflow-error runs.
    pub error: Option<String>,
}

impl RunRow {
    pub fn new(run: &Run, people: &BTreeMap<AccountId, Account>, urls: &ActionsUrls) -> Self {
        Self {
            url: urls.run(run.number),
            look: StateLook::of_run(run),
            title: run.title.clone(),
            number: run.number,
            workflow: run.workflow_name.clone(),
            event: event_label(run.event),
            reference: short_ref(&run.reference),
            actor: name_of(people, run.trigger_actor.or(run.actor)),
            when: run.created_at.relative(),
            when_iso: run.created_at.iso8601(),
            duration: if run.status.is_done() || run.status == Status::Running { elapsed(run.started_at, run.finished_at) } else { None },
            error: run.error.clone(),
        }
    }
}

/// A workflow in the sidebar.
#[derive(Debug, Clone)]
pub struct WorkflowLink {
    pub name: String,
    pub file: String,
    pub url: String,
    pub is_current: bool,
}

/// A choice in a filter `<select>`.
#[derive(Debug, Clone)]
pub struct FilterOption {
    pub value: &'static str,
    pub label: &'static str,
    pub selected: bool,
}

/// The filters as the form shows them.
#[derive(Debug, Clone, Default)]
pub struct RunFilters {
    /// The list without the workflow filter (the form's target).
    pub action: String,
    pub branch: String,
    pub actor: String,
    pub events: Vec<FilterOption>,
    pub statuses: Vec<FilterOption>,
    /// Whether any filter is set ("Clear" link).
    pub any: bool,
}

impl RunFilters {
    pub fn new(action: String, branch: &str, actor: &str, event: Option<RunEvent>, status: Option<Status>) -> Self {
        let events = RunEvent::ALL
            .into_iter()
            .map(|option| FilterOption { value: option.as_str(), label: event_label(option), selected: event == Some(option) })
            .collect();
        let statuses = Status::ALL
            .into_iter()
            .map(|option| FilterOption { value: option.as_str(), label: StateLook::of(option).label, selected: status == Some(option) })
            .collect();
        let any = !branch.is_empty() || !actor.is_empty() || event.is_some() || status.is_some();
        Self { action, branch: branch.to_owned(), actor: actor.to_owned(), events, statuses, any }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/runs.html")]
pub struct RunsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub runs: Vec<RunRow>,
    pub workflows: Vec<WorkflowLink>,
    /// The list of every workflow's runs.
    pub all_url: String,
    /// The selected workflow's name, when the list is filtered by one.
    pub workflow: Option<String>,
    pub filters: RunFilters,
    /// Older runs, when there are more.
    pub next_url: Option<String>,
    // Actions: dispatch form.
    /// "Run workflow", on a `workflow_dispatch` workflow's page for writers.
    pub dispatch: Option<DispatchFormView>,
}

// Actions: dispatch form.
/// The form that runs a workflow by hand. Choosing the branch or tag reloads the page with
/// `?ref=`, so the inputs are always those of the workflow at that ref; no JavaScript needed.
#[derive(Debug, Clone)]
pub struct DispatchFormView {
    /// Where the form posts.
    pub action: String,
    /// The workflow's page, which the ref form reloads.
    pub page_url: String,
    /// The branch or tag, as short as it was given.
    pub reference: String,
    /// Branches, then tags, offered while typing a ref.
    pub refs: Vec<String>,
    pub inputs: Vec<DispatchInputView>,
    /// Why the last dispatch, or reading the workflow, failed.
    pub error: Option<String>,
    /// Whether the workflow was read and can run (not only its error shown).
    pub can_run: bool,
    /// Open when there is something to fix or a ref was just chosen.
    pub open: bool,
    /// The workflow's `name`, when it was read: a workflow that never ran is otherwise
    /// known only by its file.
    pub name: Option<String>,
}

/// One `workflow_dispatch` input, rendered by its type.
#[derive(Debug, Clone)]
pub struct DispatchInputView {
    pub name: String,
    /// The form field: `inputs[name]`.
    pub field: String,
    /// The element id: `dispatch-input-<n>`.
    pub id: String,
    pub description: Option<String>,
    pub required: bool,
    /// `text`, `number`, `boolean` or `choice`.
    pub kind: &'static str,
    /// The value shown: what was submitted, else the default.
    pub value: String,
    pub options: Vec<DispatchOption>,
}

#[derive(Debug, Clone)]
pub struct DispatchOption {
    pub value: String,
    pub selected: bool,
}

impl DispatchInputView {
    /// The `index`th input, showing `submitted` when the form is shown again.
    pub fn new(index: usize, input: &rbit_workflow::model::DispatchInput, submitted: Option<&str>) -> Self {
        use rbit_workflow::model::InputKind;
        let value = submitted.map(str::to_owned).or_else(|| input.default.clone()).unwrap_or_default();
        let (kind, options) = match &input.kind {
            InputKind::String | InputKind::Environment => ("text", Vec::new()),
            InputKind::Number => ("number", Vec::new()),
            InputKind::Boolean => ("boolean", Vec::new()),
            InputKind::Choice(options) => {
                ("choice", options.iter().map(|option| DispatchOption { value: option.clone(), selected: *option == value }).collect())
            }
        };
        Self {
            name: input.name.clone(),
            field: format!("inputs[{}]", input.name),
            id: format!("dispatch-input-{index}"),
            description: input.description.clone().filter(|description| !description.trim().is_empty()),
            required: input.required,
            kind,
            value,
            options,
        }
    }

    /// A boolean input is ticked when its value is `true`.
    pub fn checked(&self) -> bool {
        self.value.eq_ignore_ascii_case("true")
    }
}

/// One job on the run page, indented by how deep in `needs` it is.
#[derive(Debug, Clone)]
pub struct JobRow {
    pub name: String,
    pub url: String,
    pub look: StateLook,
    pub duration: Option<String>,
    /// "needs build, lint"; empty without `needs`.
    pub needs: String,
    /// 0 for jobs without `needs`, then one more than the deepest job needed.
    pub depth: usize,
    /// The width class that indents the row by its depth (at most four levels).
    pub indent: &'static str,
    pub rerun_url: String,
    pub can_rerun: bool,
    // Actions: environments.
    /// The deployment environment the job runs in.
    pub environment: Option<String>,
    /// The URL the job reported for it (`environment.url`), shown when it is http(s).
    pub environment_url: Option<String>,
}

/// The jobs of a run in `needs` order: each after everything it needs, otherwise in the
/// workflow's order.
pub fn job_rows(jobs: &[Job], number: i64, urls: &ActionsUrls, can_write: bool) -> Vec<JobRow> {
    let mut depth: HashMap<&str, usize> = HashMap::new();
    // Jobs need only jobs of the same run, which never form a cycle, so a pass per level
    // settles every depth; anything left after as many passes as jobs is cut off at the end.
    for _ in 0..=jobs.len() {
        let mut changed = false;
        for job in jobs {
            let level = job.needs.iter().map(|need| depth.get(need.as_str()).map_or(0, |level| level + 1)).max().unwrap_or(0);
            if depth.insert(job.key.as_str(), level) != Some(level) {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut ordered: Vec<(usize, usize, &Job)> =
        jobs.iter().enumerate().map(|(index, job)| (depth[job.key.as_str()], index, job)).collect();
    ordered.sort_by_key(|(level, index, _)| (*level, *index));
    ordered
        .into_iter()
        .map(|(level, _, job)| JobRow {
            name: job.name.clone(),
            url: urls.job(number, job.id.0),
            look: StateLook::of(job.status),
            duration: elapsed(job.started_at, job.finished_at),
            needs: if job.needs.is_empty() { String::new() } else { format!("needs {}", job.needs.join(", ")) },
            depth: level,
            indent: ["", "w-4", "w-8", "w-12", "w-16"][level.min(4)],
            rerun_url: format!("{}/rerun", urls.job(number, job.id.0)),
            can_rerun: can_write && job.status.is_done(),
            environment: job.environment.clone(),
            environment_url: job.environment_url.clone().filter(|url| url.starts_with("https://") || url.starts_with("http://")),
        })
        .collect()
}

/// What the run page's heading says about the run.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub title: String,
    pub number: i64,
    pub look: StateLook,
    pub workflow: String,
    pub workflow_url: String,
    pub event: &'static str,
    pub reference: String,
    pub sha: String,
    pub short_sha: String,
    pub commit_url: String,
    pub actor: Option<String>,
    pub approved_by: Option<String>,
    pub attempt: i64,
    pub when: String,
    pub when_iso: String,
    pub duration: Option<String>,
    pub error: Option<String>,
    /// Read-only token, no secrets: a fork or an outside author.
    pub untrusted: bool,
}

impl RunSummary {
    pub fn new(run: &Run, people: &BTreeMap<AccountId, Account>, urls: &ActionsUrls, commit_url: String) -> Self {
        Self {
            title: run.title.clone(),
            number: run.number,
            look: StateLook::of_run(run),
            workflow: run.workflow_name.clone(),
            workflow_url: urls.workflow(&run.workflow_path),
            event: event_label(run.event),
            reference: short_ref(&run.reference),
            sha: run.sha.clone(),
            short_sha: run.sha.chars().take(7).collect(),
            commit_url,
            actor: name_of(people, run.trigger_actor.or(run.actor)),
            approved_by: name_of(people, run.approved_by),
            attempt: run.attempt,
            when: run.created_at.relative(),
            when_iso: run.created_at.iso8601(),
            duration: elapsed(run.started_at, run.finished_at),
            error: run.error.clone(),
            untrusted: !run.trusted,
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/run.html")]
pub struct RunPage {
    pub page: Page,
    pub header: RepoHeader,
    pub run: RunSummary,
    pub jobs: Vec<JobRow>,
    /// Not finished: the job list refreshes itself.
    pub active: bool,
    pub self_url: String,
    pub runs_url: String,
    pub can_write: bool,
    pub awaiting_approval: bool,
    /// The buttons a writer has now; `None` hides one.
    pub cancel_url: Option<String>,
    pub rerun_url: Option<String>,
    pub rerun_failed_url: Option<String>,
    pub approve_url: String,
    // Actions: artifacts (A).
    pub artifacts: Vec<ArtifactRow>,
    // Actions: step summaries.
    pub summaries: Vec<SummaryView>,
    // Actions: environments.
    /// Jobs waiting for an environment's reviewers, by environment.
    pub deployments: Vec<DeploymentView>,
    /// Where reviews post.
    pub review_url: String,
}

// Actions: environments.
/// The jobs of a run waiting for one environment's reviewers.
#[derive(Debug, Clone)]
pub struct DeploymentView {
    pub environment: String,
    /// "deploy, smoke test".
    pub jobs: String,
    pub can_review: bool,
}

// Actions: step summaries.
/// What a job's steps wrote to `GITHUB_STEP_SUMMARY`, rendered like any untrusted Markdown
/// (sanitised; raw HTML never reaches the page).
#[derive(Debug, Clone)]
pub struct SummaryView {
    pub job: String,
    pub html: String,
}

impl SummaryView {
    pub fn new(job: &str, texts: &[String]) -> Self {
        Self { job: job.to_owned(), html: crate::render::render_markdown(&texts.join("\n"), None) }
    }
}

// Actions: artifacts (A).
/// An artifact on the run page.
#[derive(Debug, Clone)]
pub struct ArtifactRow {
    pub name: String,
    pub size: String,
    pub expires: String,
    pub expires_iso: String,
    pub url: String,
}

impl ArtifactRow {
    pub fn new(artifact: &rbit_core::actions::Artifact, urls: &ActionsUrls, number: i64) -> Self {
        Self {
            name: artifact.name.clone(),
            size: crate::render::format_size(artifact.size),
            expires: artifact.expires_at.until(),
            expires_iso: artifact.expires_at.iso8601(),
            url: format!("{}/artifacts/{}", urls.run(number), artifact.id.0),
        }
    }
}

// ------------------------------------------------------------------ the job page

/// Lines of one step shown on the job page; the step's log page shows up to
/// `rbit_services::actions::logs::STEP_VIEW_LINES`, the raw log everything.
pub const JOB_PAGE_STEP_LINES: usize = 1000;
/// Bytes of one step shown on the job page.
pub const JOB_PAGE_STEP_BYTES: usize = 256 * 1024;

/// `line` without ANSI escape sequences (colours, cursor moves, OSC titles and links):
/// logs are shown as text, never as terminal output.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI: parameters and intermediates, then one final byte in `@`..=`~`.
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: until BEL or ESC `\`.
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            // Any other escape is two characters long.
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// One log line as HTML: escaped, without ANSI escapes, with the runner's workflow command
/// markers (`##[group]`, `##[error]` …, see rbit-runner's `commands`) as text and tone.
pub fn log_line_html(line: &str) -> String {
    let line = strip_ansi(line);
    let markers: [(&str, &str, &str); 6] = [
        ("##[group]", "font-semibold text-ink", ""),
        ("##[endgroup]", "text-ink-faint", ""),
        ("##[error]", "text-danger", "Error: "),
        ("##[warning]", "text-warning", "Warning: "),
        ("##[notice]", "text-ink", "Notice: "),
        ("##[debug]", "text-ink-faint", ""),
    ];
    for (marker, tone, label) in markers {
        if let Some(rest) = line.strip_prefix(marker) {
            return format!(r#"<span class="{tone}">{label}{}</span>"#, escape_html(rest));
        }
    }
    escape_html(&line)
}

/// A step's log lines, ready for `repo/code` blocks.
#[derive(Debug, Clone, Default)]
pub struct StepLog {
    /// Escaped HTML, one per line.
    pub lines: Vec<String>,
    /// The step-relative number of `lines[0]`, from 1.
    pub first: usize,
    /// Earlier lines were left out (see the step's log page or the raw log).
    pub more_before: bool,
    pub truncated: bool,
    pub expired: bool,
}

impl StepLog {
    /// `window` of a step whose log starts at task-log line `step_start`.
    pub fn new(window: &LogWindow, step_start: u64) -> Self {
        let first = usize::try_from(window.first_line.saturating_sub(step_start)).unwrap_or(0) + 1;
        Self {
            lines: window.lines.iter().map(|line| log_line_html(line)).collect(),
            first,
            more_before: window.more_before,
            truncated: window.truncated,
            expired: window.expired,
        }
    }

    pub fn blocks(&self) -> Vec<Block<CodeLine<'_>>> {
        blocks(self.lines.iter().enumerate().map(|(index, html)| CodeLine { number: self.first + index, html }))
    }

    pub fn columns(&self) -> String {
        column_class(self.lines.iter().map(|line| markup_columns(line)).max().unwrap_or(0))
    }
}

/// One step on the job page.
#[derive(Debug, Clone)]
pub struct StepView {
    pub number: i64,
    pub name: String,
    pub look: StateLook,
    pub duration: Option<String>,
    /// Shown expanded: the running step, else the first that failed.
    pub open: bool,
    /// `None` for a step that never started.
    pub log: Option<StepLog>,
    /// The step's own log page.
    pub log_url: String,
}

impl StepView {
    pub fn new(step: &Step, log: Option<StepLog>, log_url: String) -> Self {
        Self {
            number: step.number,
            name: step.name.clone(),
            look: StateLook::of(step.status),
            duration: elapsed(step.started_at, step.stopped_at),
            open: false,
            log,
            log_url,
        }
    }
}

/// Opens the running step, or else the first failed one.
pub fn open_one(steps: &mut [StepView]) {
    let running = steps.iter().position(|step| step.look.key == "running");
    let failed = steps.iter().position(|step| step.look.key == "failure");
    if let Some(index) = running.or(failed) {
        steps[index].open = true;
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/job.html")]
pub struct JobPage {
    pub page: Page,
    pub header: RepoHeader,
    pub run: RunSummary,
    pub run_url: String,
    pub name: String,
    pub look: StateLook,
    pub duration: Option<String>,
    pub labels: String,
    /// Not finished: the page refreshes itself.
    pub active: bool,
    pub self_url: String,
    pub steps: Vec<StepView>,
    /// `None` until a runner took the job.
    pub raw_url: Option<String>,
    pub rerun_url: Option<String>,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/step_log.html")]
pub struct StepLogPage {
    pub page: Page,
    pub header: RepoHeader,
    pub job_name: String,
    pub job_url: String,
    pub step: StepView,
    pub raw_url: String,
    pub active: bool,
    pub self_url: String,
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        RepoId,
        actions::{JobId, RunId},
    };

    use super::*;

    fn job(id: i64, key: &str, needs: &[&str], status: Status) -> Job {
        Job {
            id: JobId(id),
            run_id: RunId(1),
            repo_id: RepoId(1),
            key: key.to_owned(),
            name: key.to_owned(),
            matrix: None,
            needs: needs.iter().map(|need| (*need).to_owned()).collect(),
            runs_on: vec!["x".to_owned()],
            permissions: None,
            status,
            attempt: 1,
            task_id: None,
            runner_id: None,
            created_at: Timestamp(0),
            queued_at: None,
            started_at: None,
            finished_at: None,
            environment: None,
            environment_url: None,
            awaiting_review: false,
        }
    }

    #[test]
    fn jobs_come_after_what_they_need() {
        let jobs = [
            job(1, "deploy", &["test", "lint"], Status::Blocked),
            job(2, "test", &["build"], Status::Blocked),
            job(3, "build", &[], Status::Success),
            job(4, "lint", &[], Status::Failure),
        ];
        let urls = ActionsUrls::new("/a/b");
        let rows = job_rows(&jobs, 7, &urls, true);
        let order: Vec<(&str, usize)> = rows.iter().map(|row| (row.name.as_str(), row.depth)).collect();
        assert_eq!(order, [("build", 0), ("lint", 0), ("test", 1), ("deploy", 2)]);
        assert_eq!(rows[0].url, "/a/b/actions/runs/7/jobs/3");
        assert_eq!(rows[3].needs, "needs test, lint");
        assert!(rows[0].can_rerun && !rows[2].can_rerun, "only finished jobs run again");
        assert!(job_rows(&jobs, 7, &urls, false).iter().all(|row| !row.can_rerun));
    }

    #[test]
    fn refs_and_workflows_are_shortened() {
        assert_eq!(short_ref("refs/heads/feature/x"), "feature/x");
        assert_eq!(short_ref("refs/pull/12/head"), "#12");
        assert_eq!(short_ref("refs/tags/v1.0"), "v1.0");
        assert_eq!(workflow_file(".rbit/workflows/ci.yml"), "ci.yml");
        assert_eq!(ActionsUrls::new("/a/b").workflow(".github/workflows/release.yaml"), "/a/b/actions/workflows/release.yaml");
    }

    #[test]
    fn logs_are_text_with_their_markers_shown() {
        assert_eq!(strip_ansi("\u{1b}[1;31mred\u{1b}[0m and \u{1b}]8;;https://x\u{7}link\u{1b}]8;;\u{1b}\\ done"), "red and link done");
        assert_eq!(strip_ansi("plain \u{1b}"), "plain ");
        assert_eq!(log_line_html("<b>\u{1b}[32mok</b>"), "&lt;b&gt;ok&lt;/b&gt;");
        assert_eq!(log_line_html("##[error]it <broke>"), r#"<span class="text-danger">Error: it &lt;broke&gt;</span>"#);
        assert_eq!(log_line_html("##[group]Install"), r#"<span class="font-semibold text-ink">Install</span>"#);
        assert_eq!(log_line_html("not ##[error] a marker"), "not ##[error] a marker");
    }

    #[test]
    fn the_running_step_or_else_the_first_failure_is_open() {
        let step = |number: i64, status: Status| {
            let step = Step {
                task_id: rbit_core::actions::TaskId(1),
                number,
                name: format!("s{number}"),
                status,
                started_at: None,
                stopped_at: None,
                log_index: 0,
                log_length: 0,
            };
            StepView::new(&step, None, String::new())
        };
        let mut steps = vec![step(0, Status::Success), step(1, Status::Failure), step(2, Status::Failure)];
        open_one(&mut steps);
        assert_eq!(steps.iter().map(|step| step.open).collect::<Vec<_>>(), [false, true, false]);
        let mut steps = vec![step(0, Status::Failure), step(1, Status::Running)];
        open_one(&mut steps);
        assert_eq!(steps.iter().map(|step| step.open).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn every_state_has_a_look() {
        for status in Status::ALL {
            let look = StateLook::of(status);
            assert!(!look.key.is_empty() && !look.label.is_empty() && look.tone.starts_with("text-"));
        }
    }
}
