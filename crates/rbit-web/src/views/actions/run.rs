//! Run presentation: existing job ordering, metadata, review, artifacts and summaries.

use super::{ActionsUrls, StateLook, elapsed, event_label, name_of, short_ref};
use crate::views::{layout::Page, repo::RepoHeader};
use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{
    Account, AccountId,
    actions::{Job, Run},
};
use std::collections::{BTreeMap, HashMap};

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
