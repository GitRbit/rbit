//! Run presentation: existing job ordering, metadata, review, artifacts and summaries.

use super::{ActionsUrls, StateLook, elapsed, event_label, event_message, name_of, short_ref};
use crate::views::when::{Took, When};
use crate::views::{layout::Page, repo::RepoHeader};
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Account, AccountId,
    actions::{Job, Run},
};
use std::collections::BTreeMap;

/// One job on the run page, indented by how deep in `needs` it is.
#[derive(Debug, Clone)]
pub struct JobRow {
    pub name: String,
    pub url: String,
    pub look: StateLook,
    pub duration: Option<Took>,
    /// "build, lint": the jobs it needs; empty without `needs`.
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
    super::run_layout::needs_order(jobs)
        .into_iter()
        .map(|(level, job)| JobRow {
            name: job.name.clone(),
            url: urls.job(number, job.id.0),
            look: StateLook::of(job.status),
            duration: elapsed(job.started_at, job.finished_at),
            needs: job.needs.join(", "),
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
    /// The event in the page's language.
    pub event_message: rbit_localization::OverviewControlMessage,
    pub reference: String,
    pub sha: String,
    pub short_sha: String,
    pub commit_url: String,
    pub actor: Option<String>,
    pub approved_by: Option<String>,
    pub attempt: i64,
    pub when: When,
    pub when_iso: String,
    pub duration: Option<Took>,
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
            event_message: event_message(run.event),
            reference: short_ref(&run.reference),
            sha: run.sha.clone(),
            short_sha: run.sha.chars().take(7).collect(),
            commit_url,
            actor: name_of(people, run.trigger_actor.or(run.actor)),
            approved_by: name_of(people, run.approved_by),
            attempt: run.attempt,
            when: When(run.created_at),
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
    /// The run's navigation beside the page.
    pub sidebar: super::RunSidebar,
    /// The jobs and their `needs` as a graph; `None` without jobs.
    pub graph: Option<super::JobGraph>,
    /// The errors, warnings and notices of the run's jobs.
    pub annotations: super::AnnotationsView,
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

/// The run section and the run's job list alone, as the run section's poll gets them; the
/// page without its frame.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "actions/run/live.html")]
pub struct RunPageLive(pub RunPage);

impl std::ops::Deref for RunPageLive {
    type Target = RunPage;

    fn deref(&self) -> &RunPage {
        &self.0
    }
}

// Actions: environments.
/// The jobs of a run waiting for one environment's reviewers.
#[derive(Debug, Clone)]
pub struct DeploymentView {
    pub environment: String,
    /// "deploy, smoke test".
    pub jobs: String,
    /// How many jobs `jobs` names.
    pub job_count: u64,
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
    pub size: crate::views::when::Size,
    pub expires: When,
    pub expires_iso: String,
    pub url: String,
}

impl ArtifactRow {
    pub fn new(artifact: &rbit_core::actions::Artifact, urls: &ActionsUrls, number: i64) -> Self {
        Self {
            name: artifact.name.clone(),
            size: crate::views::when::Size(artifact.size),
            expires: When(artifact.expires_at),
            expires_iso: artifact.expires_at.iso8601(),
            url: format!("{}/artifacts/{}", urls.run(number), artifact.id.0),
        }
    }
}

impl RunPage {
    pub fn labels(&self) -> super::RunText {
        super::RunText::new(self.page.locale)
    }

    pub fn rerun_label<'a>(&self, name: &'a str) -> rbit_localization::ActionsDisplay<'a> {
        rbit_localization::Catalog::new(self.page.locale).actions_display(rbit_localization::ActionsMessage::RerunJob { name })
    }

    pub fn summary_label<'a>(&self, job: &'a str) -> rbit_localization::ActionsDisplay<'a> {
        rbit_localization::Catalog::new(self.page.locale).actions_display(rbit_localization::ActionsMessage::Summary { job })
    }
}
