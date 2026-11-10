//! The run layout the run and job pages share: beside the page, the run's navigation (its
//! summary, every job with its state and duration, how long the jobs took together and the
//! workflow file), as GitHub shows it.
//!
//! While the run is not finished the job list refreshes itself: on the run page with the
//! run section's poll (`hx-select-oob`), on a job page from `GET …/runs/{n}/sidebar`.
//! Running durations tick in the browser (`assets/actions-run.js`) from `data-since`.
use std::collections::HashMap;

use rbit_core::{
    Timestamp,
    actions::{Job, JobId, Run, Status},
};
use rbit_localization::{Catalog, Locale, Localized, RunLayoutMessage};

use super::{
    ActionsUrls, StateLook, elapsed,
    job_text::{LocalizedState, localized_state},
};
use crate::views::when::Took;

/// How deep each job is in `needs`, by key: 0 without `needs`, then one more than the
/// deepest job it needs. Jobs need only jobs of the same run, which never form a cycle, so
/// a pass per level settles every depth; anything left after as many passes as jobs is
/// cut off there.
pub fn needs_depths(jobs: &[Job]) -> HashMap<&str, usize> {
    let mut depth: HashMap<&str, usize> = HashMap::new();
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
    depth
}

/// The jobs in `needs` order: each after everything it needs, otherwise in the workflow's
/// order, with their depth.
pub fn needs_order(jobs: &[Job]) -> Vec<(usize, &Job)> {
    let depth = needs_depths(jobs);
    let mut ordered: Vec<(usize, usize, &Job)> =
        jobs.iter().enumerate().map(|(index, job)| (depth.get(job.key.as_str()).copied().unwrap_or(0), index, job)).collect();
    ordered.sort_by_key(|(level, index, _)| (*level, *index));
    ordered.into_iter().map(|(level, _, job)| (level, job)).collect()
}

/// One state for several jobs (a matrix): what needs attention first.
pub fn combined_status(statuses: impl IntoIterator<Item = Status>) -> Status {
    const ORDER: [Status; 8] = [
        Status::Failure,
        Status::Running,
        Status::AwaitingApproval,
        Status::Waiting,
        Status::Blocked,
        Status::Cancelled,
        Status::Success,
        Status::Skipped,
    ];
    let statuses: Vec<Status> = statuses.into_iter().collect();
    ORDER.into_iter().find(|status| statuses.contains(status)).unwrap_or(Status::Skipped)
}

/// A job's duration so far and, while it runs, when it started (for the ticking clock).
pub fn job_clock(job: &Job) -> (Option<Took>, Option<i64>) {
    let since = (job.status == Status::Running).then_some(job.started_at).flatten().map(Timestamp::seconds);
    (elapsed(job.started_at, job.finished_at), since)
}

/// One job in the run's navigation.
#[derive(Debug, Clone)]
pub struct SidebarJob {
    pub name: String,
    pub url: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    /// While it runs: when it started, in Unix seconds.
    pub since: Option<i64>,
    /// The page shows this job.
    pub current: bool,
}

/// The jobs of one workflow job key: one job, or the jobs its matrix made, under the key.
#[derive(Debug, Clone)]
pub struct SidebarGroup {
    /// The key for a matrix, `None` for a single job.
    pub name: Option<String>,
    pub look: StateLook,
    pub count: u64,
    pub jobs: Vec<SidebarJob>,
}

/// The run's navigation beside the run and job pages.
#[derive(Debug, Clone)]
pub struct RunSidebar {
    pub locale: Locale,
    pub summary_url: String,
    /// The page is the run's summary.
    pub on_summary: bool,
    /// What the page shows (the summary or a job's name): the phone disclosure's label.
    pub current_name: Option<String>,
    pub groups: Vec<SidebarGroup>,
    /// How long the jobs ran together.
    pub usage: Option<Took>,
    pub workflow_path: String,
    /// The workflow file at the run's commit.
    pub workflow_file_url: String,
    /// While the run is not finished on a job page: where the job list refreshes from.
    pub poll_url: Option<String>,
    /// The server's clock, in Unix seconds, so durations tick from the same time.
    pub now: i64,
}

impl RunSidebar {
    /// The navigation of `run` with `jobs`; `current` is the job the page shows, `None` on
    /// the summary. `poll` makes the job list refresh itself while the run is not finished.
    pub fn new(run: &Run, jobs: &[Job], urls: &ActionsUrls, workflow_file_url: String, current: Option<JobId>, locale: Locale) -> Self {
        let mut groups: Vec<SidebarGroup> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut statuses: Vec<Vec<Status>> = Vec::new();
        for (_, job) in needs_order(jobs) {
            let (duration, since) = job_clock(job);
            let entry = SidebarJob {
                name: job.name.clone(),
                url: urls.job(run.number, job.id.0),
                look: StateLook::of(job.status),
                duration,
                since,
                current: current == Some(job.id),
            };
            let slot = *index.entry(job.key.as_str()).or_insert_with(|| {
                groups.push(SidebarGroup { name: None, look: entry.look, count: 0, jobs: Vec::new() });
                statuses.push(Vec::new());
                groups.len() - 1
            });
            if job.matrix.is_some() {
                groups[slot].name = Some(job.key.clone());
            }
            groups[slot].jobs.push(entry);
            groups[slot].count += 1;
            statuses[slot].push(job.status);
        }
        for (group, statuses) in groups.iter_mut().zip(statuses) {
            group.look = StateLook::of(combined_status(statuses));
        }
        let usage: i64 = jobs.iter().filter_map(|job| elapsed(job.started_at, job.finished_at)).map(|took| took.0.max(0)).sum();
        Self {
            locale,
            summary_url: urls.run(run.number),
            on_summary: current.is_none(),
            current_name: current.and_then(|id| jobs.iter().find(|job| job.id == id)).map(|job| job.name.clone()),
            groups,
            usage: jobs.iter().any(|job| job.started_at.is_some()).then_some(Took(usage)),
            workflow_path: run.workflow_path.clone(),
            workflow_file_url,
            poll_url: None,
            now: Timestamp::now().seconds(),
        }
    }

    /// Makes the job list refresh itself from `url` while `run` is not finished.
    #[must_use]
    pub fn polling(mut self, run: &Run, url: String) -> Self {
        self.poll_url = (!run.status.is_done()).then_some(url);
        self
    }

    pub fn text(&self) -> RunLayoutText {
        RunLayoutText::new(self.locale)
    }

    /// `1h 2m`, `3m 4s` and `5s` with 11, 22, 33, 44 and 55 in place of the numbers: the
    /// shapes a ticking duration takes in the page's language.
    pub fn clock_patterns(&self) -> [String; 3] {
        [40_920, 2_024, 55].map(|seconds| Took(seconds).text(self.locale).as_str().to_owned())
    }
}

/// The run layout's labels in the page's language.
#[derive(Debug)]
pub struct RunLayoutText {
    catalog: Catalog,
    pub summary: Localized,
    pub navigation: Localized,
    pub jobs: Localized,
    pub details: Localized,
    pub usage: Localized,
    pub workflow_file: Localized,
}

impl RunLayoutText {
    pub fn new(locale: Locale) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
            summary: catalog.localized(RunLayoutMessage::Summary),
            navigation: catalog.localized(RunLayoutMessage::Navigation),
            jobs: catalog.localized(rbit_localization::ActionsMessage::Jobs),
            details: catalog.localized(rbit_localization::ActionsMessage::RunDetails),
            usage: catalog.localized(RunLayoutMessage::Usage),
            workflow_file: catalog.localized(RunLayoutMessage::WorkflowFile),
        }
    }

    /// A state icon's label in the page's locale.
    pub fn look(&self, look: &StateLook) -> LocalizedState {
        localized_state(self.catalog, look)
    }

    /// "3 jobs": the size of a matrix.
    pub fn job_count(&self, count: &u64) -> Localized {
        self.catalog.localized(RunLayoutMessage::JobCount { count: *count })
    }
}

/// The job list alone, as `GET …/runs/{n}/sidebar` answers a job page's refresh.
#[derive(Debug, askama::Template, askama_web::WebTemplate)]
#[template(path = "actions/run/sidebar_jobs.html")]
pub struct RunSidebarJobs {
    pub sidebar: RunSidebar,
}

#[cfg(test)]
#[path = "run_layout_tests.rs"]
pub(super) mod tests;
