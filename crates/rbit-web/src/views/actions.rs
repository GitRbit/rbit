//! rbit Actions pages: a repository's runs, one run with its jobs, and one job with its
//! steps and their logs.
//!
//! Colour marks state only (`text-success`, `text-danger`, `text-warning`); everything else
//! stays monochrome. Every page works without JavaScript: the runs list, the run page and
//! the job page refresh themselves with htmx while something on them is not finished, and a
//! reload does the same. `assets/actions-log.js` keeps the job page's steps open or closed
//! as the viewer left them across those refreshes.
//!
//! Workstream U (interface and secrets) owns this file and `templates/actions/**`.

use std::collections::BTreeMap;

use crate::views::when::Took;
use rbit_core::{
    Account, AccountId, Timestamp,
    actions::{Job, Run, RunEvent, Status},
};

mod ansi;
mod job;
mod job_sections;
mod job_text;
pub mod log_lines;
pub use job::{Failure, JOB_PAGE_STEP_BYTES, JOB_PAGE_STEP_LINES, JobPage, StepLog, StepLogPage, StepView, open_one};
pub use job_sections::{Section, growing, section_view};
pub use job_text::JobText;

mod overview_text;
pub use overview_text::OverviewText;
mod overview;
pub use overview::{FilterOption, RunFilters, RunRow, RunsPage, WorkflowLink, runs_refresh};
mod dispatch;
pub use dispatch::{DispatchFormView, DispatchInputView, DispatchOption};

mod run;
mod run_text;
pub use run::{ArtifactRow, DeploymentView, JobRow, RunPage, RunPageLive, RunSummary, SummaryView, job_rows, name_runners};
pub use run_text::RunText;
mod run_annotations;
pub use run_annotations::AnnotationsView;
mod run_graph;
mod run_trigger;
pub use run_graph::JobGraph;
pub use run_trigger::TriggerView;
mod run_layout;
pub use run_layout::{RunLayoutText, RunSidebar, RunSidebarJobs, combined_status, needs_depths, needs_order};
pub mod environment_text;
mod runners_text;
mod settings_text;
pub mod ui_text;

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
        RunEvent::WorkflowRun => "after a workflow",
        RunEvent::RepositoryDispatch => "dispatch",
    }
}

/// The event as the catalogs name it; the same words as [`event_label`] in English.
pub const fn event_message(event: RunEvent) -> rbit_localization::OverviewControlMessage {
    use rbit_localization::OverviewControlMessage as Control;
    match event {
        RunEvent::Push => Control::Push,
        RunEvent::PullRequest => Control::PullRequest,
        RunEvent::WorkflowDispatch => Control::Manual,
        RunEvent::Schedule => Control::Schedule,
        RunEvent::WorkflowRun => Control::WorkflowRun,
        RunEvent::RepositoryDispatch => Control::Dispatch,
    }
}

/// How long something took: from its start to its end, or to now while it runs.
fn elapsed(started: Option<Timestamp>, finished: Option<Timestamp>) -> Option<Took> {
    let started = started?;
    let end = finished.unwrap_or_else(Timestamp::now);
    Some(Took(end.seconds() - started.seconds()))
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
pub fn job_duration(job: &Job) -> Option<Took> {
    elapsed(job.started_at, job.finished_at)
}

// ------------------------------------------------------------------ status badges

// Actions: workflow status badges.
/// A workflow's status badge: the label on grey, then "passing" on green, "failing" on red
/// or "no status" on grey, in the flat style READMEs show next to each other.
pub fn badge_svg(label: &str, passing: Option<bool>) -> String {
    let (message, color) = match passing {
        Some(true) => ("passing", "#2ea44f"),
        Some(false) => ("failing", "#cb2431"),
        None => ("no status", "#9f9f9f"),
    };
    let label: String = label.chars().filter(|c| !c.is_control()).take(64).collect();
    let (label_width, message_width) = (badge_text_width(&label) + 12, badge_text_width(message) + 12);
    let width = label_width + message_width;
    let (label_x, message_x) = (f64::from(label_width) / 2.0, f64::from(label_width) + f64::from(message_width) / 2.0);
    let label = escape_xml(&label);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"20\" role=\"img\" aria-label=\"{label}: {message}\">\
         <title>{label}: {message}</title>\
         <linearGradient id=\"s\" x2=\"0\" y2=\"100%\"><stop offset=\"0\" stop-color=\"#bbb\" stop-opacity=\".1\"/><stop offset=\"1\" stop-opacity=\".1\"/></linearGradient>\
         <clipPath id=\"r\"><rect width=\"{width}\" height=\"20\" rx=\"3\" fill=\"#fff\"/></clipPath>\
         <g clip-path=\"url(#r)\"><rect width=\"{label_width}\" height=\"20\" fill=\"#555\"/>\
         <rect x=\"{label_width}\" width=\"{message_width}\" height=\"20\" fill=\"{color}\"/><rect width=\"{width}\" height=\"20\" fill=\"url(#s)\"/></g>\
         <g fill=\"#fff\" text-anchor=\"middle\" font-family=\"Verdana,Geneva,DejaVu Sans,sans-serif\" font-size=\"11\">\
         <text x=\"{label_x}\" y=\"15\" fill=\"#010101\" fill-opacity=\".3\">{label}</text><text x=\"{label_x}\" y=\"14\">{label}</text>\
         <text x=\"{message_x}\" y=\"15\" fill=\"#010101\" fill-opacity=\".3\">{message}</text><text x=\"{message_x}\" y=\"14\">{message}</text></g></svg>"
    )
}

/// The width of `text` in 11px Verdana, near enough to size a badge: glyph widths by class.
fn badge_text_width(text: &str) -> u32 {
    let tenths: u32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' | 'I' => 35,
            ' ' | 'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' | '/' => 45,
            'm' | 'w' | 'M' | 'W' | '%' | '@' => 100,
            'A'..='Z' => 75,
            '0'..='9' => 70,
            c if c.is_ascii() => 65,
            _ => 90,
        })
        .sum();
    tenths.div_ceil(10)
}

fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use rbit_core::actions::{Job, Step};
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
        assert_eq!(rows[3].needs, "test, lint");
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

#[cfg(test)]
mod badge_tests {
    use super::badge_svg;

    #[test]
    fn badges_say_how_the_workflow_fares() {
        let passing = badge_svg("CI", Some(true));
        assert!(passing.starts_with("<svg") && passing.contains(">passing</text>") && passing.contains("#2ea44f"));
        assert!(badge_svg("CI", Some(false)).contains(">failing</text>"));
        assert!(badge_svg("CI", None).contains(">no status</text>"));
    }

    #[test]
    fn labels_cannot_inject_markup() {
        let svg = badge_svg("<script>alert(1)</script> & \"x\"", Some(true));
        assert!(!svg.contains("<script>") && svg.contains("&lt;script&gt;") && svg.contains("&amp; &quot;x&quot;"));
        let long = badge_svg(&"a".repeat(500), None);
        assert!(long.len() < 4000, "labels are cut");
    }

    #[test]
    fn wider_labels_make_wider_badges() {
        let width = |svg: String| svg.split("width=\"").nth(1).unwrap().split('"').next().unwrap().parse::<u32>().unwrap();
        assert!(width(badge_svg("Release pipeline", Some(true))) > width(badge_svg("CI", Some(true))));
    }
}
