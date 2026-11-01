//! The runs list, a run page and what writers do to runs: cancel, re-run (all, failed, one
//! job) and approve; and on a workflow's page, running it by hand (`workflow_dispatch`).
//!
//! Every page needs a reader of a repository with Actions on (`ActionsService::list_runs`
//! and `run_detail` answer 404 otherwise); the buttons post to handlers the service checks
//! for write access again.
//!
//! Workstream U (interface and secrets) owns this file.

use axum::extract::{Path, Query, State};
use std::collections::BTreeMap;

use rbit_core::actions::{RunEvent, RunId, Status};
use rbit_git::RefKind;
use rbit_services::{
    ActionsService, ServiceError,
    actions::{browse::RunQuery, dispatch::DispatchRequest},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    routes::repo::RepoPage,
    views::{
        actions::{
            ActionsUrls, ArtifactRow, DeploymentView, DispatchFormView, DispatchInputView, RunFilters, RunPage, RunPagination, RunRow,
            RunSummary, RunsPage, SummaryView, WorkflowLink, job_rows, runs_refresh, workflow_file,
        },
        layout::Page,
    },
};

pub(crate) fn actions(state: &AppState) -> WebResult<ActionsService> {
    state.services.actions.snapshot().ok_or(WebError::NotFound)
}

/// The filters as GET parameters; empty values mean "any".
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    pub(crate) branch: String,
    pub(crate) actor: String,
    pub(crate) event: String,
    pub(crate) status: String,
    pub(crate) before: Option<i64>,
    // Actions: dispatch form.
    /// The branch or tag the dispatch form reads the workflow at.
    #[serde(rename = "ref")]
    pub(crate) reference: String,
}

impl ListQuery {
    fn event(&self) -> Option<RunEvent> {
        RunEvent::parse(&self.event)
    }

    fn status(&self) -> Option<Status> {
        Status::parse(&self.status)
    }

    /// The query string of these filters (without the page), for the next page's link.
    fn encoded(&self) -> String {
        let pairs = [
            ("branch", self.branch.as_str()),
            ("actor", self.actor.as_str()),
            ("event", self.event.as_str()),
            ("status", self.status.as_str()),
        ];
        pairs
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(key, value)| format!("{key}={}", percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

// Actions: dispatch form.
/// A dispatch that failed, shown again with its values and why.
pub(crate) struct Submitted {
    reference: String,
    inputs: BTreeMap<String, String>,
    error: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn runs_page(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    owner: &str,
    repo: &str,
    workflow: Option<&str>,
    query: &ListQuery,
    submitted: Option<&Submitted>,
) -> WebResult<RunsPage> {
    let actions = actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    let urls = ActionsUrls::new(&loaded.urls.home());
    // The workflows with runs, then the default branch's that never ran (named by file
    // until they do); a file name names one of them.
    let mut known = actions.workflows(&loaded.ctx).await?;
    for path in actions.workflow_paths(&loaded.ctx).await? {
        if !known.iter().any(|(known, _)| *known == path) {
            let name = workflow_file(&path).to_owned();
            known.push((path, name));
        }
    }
    let selected = match workflow {
        Some(file) => Some(known.iter().find(|(path, _)| workflow_file(path) == file).ok_or(WebError::NotFound)?.clone()),
        None => None,
    };
    // Actions: disabled workflows.
    let disabled = match &selected {
        Some((path, _)) => actions.workflow_state(&loaded.ctx, path).await?.disabled,
        None => false,
    };
    let switch_url = selected.as_ref().filter(|_| loaded.ctx.can_write_content()).map(|_| urls.run_action(0, "switch"));
    let dispatch = match &selected {
        Some((path, _)) if loaded.ctx.can_write_content() && !disabled => {
            dispatch_form(&actions, &loaded, &urls, path, query, submitted).await?
        }
        _ => None,
    };
    let run_query = RunQuery {
        workflow: selected.as_ref().map(|(path, _)| path.clone()),
        branch: non_empty(&query.branch),
        event: query.event(),
        status: query.status(),
        actor: non_empty(&query.actor),
        before: query.before.map(RunId),
    };
    let listing = actions.list_runs(&loaded.ctx, &run_query).await?;
    let action = selected.as_ref().map_or_else(|| urls.runs(), |(path, _)| urls.workflow(path));
    let page_url = |before: Option<i64>| {
        let filters = query.encoded();
        let params: Vec<String> = [filters, before.map(|before| format!("before={before}")).unwrap_or_default()]
            .into_iter()
            .filter(|param| !param.is_empty())
            .collect();
        if params.is_empty() {
            action.clone()
        } else {
            format!("{action}{}{}", if action.contains('?') { "&" } else { "?" }, params.join("&"))
        }
    };
    let next_url = listing.next.map(|before| page_url(Some(before.0)));
    let self_url = page_url(query.before);
    // The name the workflow file gives itself, when the form read it.
    let selected = selected.map(|(path, name)| (path, dispatch.as_ref().and_then(|form| form.name.clone()).unwrap_or(name)));
    let workflows = known
        .iter()
        .map(|(path, name)| {
            let current = selected.as_ref().filter(|(selected, _)| selected == path);
            WorkflowLink {
                name: current.map_or_else(|| name.clone(), |(_, name)| name.clone()),
                file: path.clone(),
                url: urls.workflow(path),
                is_current: current.is_some(),
            }
        })
        .collect();
    let title = match &selected {
        Some((_, name)) => format!("{name} · Actions · {}", loaded.ctx.repo.full_name()),
        None => format!("Actions · {}", loaded.ctx.repo.full_name()),
    };
    Ok(RunsPage {
        workflow_file: selected.as_ref().map_or_else(String::new, |(path, _)| workflow_file(path).to_owned()),
        page: page.titled(title),
        header: loaded.header(state, "actions"),
        runs: listing.runs.iter().map(|run| RunRow::new(run, &listing.people, &urls)).collect(),
        workflows,
        all_url: urls.runs(),
        workflow: selected.map(|(_, name)| name),
        refresh: runs_refresh(&listing.runs, query.before.is_none()),
        filters: RunFilters::new(action, query.branch.trim(), query.actor.trim(), query.event(), query.status()),
        next_url,
        dispatch,
        disabled,
        switch_url,
        self_url,
    })
}

// Actions: dispatch form.
/// Most branches, and then tags, offered while typing the ref to run a workflow at.
const DISPATCH_REFS: usize = 50;

/// The "Run workflow" form of workflow `path`, when it runs on `workflow_dispatch`.
async fn dispatch_form(
    actions: &ActionsService,
    loaded: &RepoPage,
    urls: &ActionsUrls,
    path: &str,
    query: &ListQuery,
    submitted: Option<&Submitted>,
) -> WebResult<Option<DispatchFormView>> {
    let reference = submitted.map_or_else(|| query.reference.trim(), |submitted| submitted.reference.trim());
    let (form, error) = match actions.dispatch_form(&loaded.ctx, path, reference).await {
        Ok(Some(form)) => (Some(form), None),
        // Not a workflow_dispatch workflow (at this ref): no form, unless one was asked for.
        Ok(None) if reference.is_empty() && submitted.is_none() => return Ok(None),
        Ok(None) => (None, Some(format!("{} does not run on workflow_dispatch at {reference}", workflow_file(path)))),
        Err(ServiceError::Invalid(message)) => (None, Some(message)),
        Err(error) => return Err(error.into()),
    };
    let shown = form.as_ref().map_or(reference, |form| form.reference.as_str());
    let shown = shown.strip_prefix("refs/heads/").or_else(|| shown.strip_prefix("refs/tags/")).unwrap_or(shown).to_owned();
    let mut refs: Vec<String> = loaded
        .refs()
        .iter()
        .filter(|summary| summary.kind == RefKind::Branch)
        .take(DISPATCH_REFS)
        .map(|summary| summary.name.clone())
        .collect();
    refs.extend(
        loaded.refs().iter().filter(|summary| summary.kind == RefKind::Tag).take(DISPATCH_REFS).map(|summary| summary.name.clone()),
    );
    let inputs = form
        .as_ref()
        .map(|form| {
            let given = |name: &str| submitted.and_then(|submitted| submitted.inputs.get(name)).map(String::as_str);
            form.inputs.iter().enumerate().map(|(index, input)| DispatchInputView::new(index, input, given(&input.name))).collect()
        })
        .unwrap_or_default();
    let error = submitted.map(|submitted| submitted.error.clone()).or(error);
    Ok(Some(DispatchFormView {
        workflow_file: workflow_file(path).to_owned(),
        action: urls.run_action(0, "dispatch"),
        page_url: urls.workflow(path),
        reference: shown,
        refs,
        inputs,
        open: error.is_some() || !query.reference.trim().is_empty(),
        error,
        can_run: form.is_some(),
        name: form.map(|form| form.name),
    }))
}

pub(crate) enum DispatchResult {
    Run(i64),
    Invalid(Box<RunsPage>),
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_native(
    state: &AppState,
    user: &rbit_core::User,
    viewer: &Viewer,
    page: Page,
    owner: &str,
    repo: &str,
    workflow: &str,
    fields: Vec<(String, String)>,
) -> WebResult<DispatchResult> {
    let actions = actions(state)?;
    let ctx = state.services.repositories.open(viewer.user(), owner, repo).await?;
    let mut reference = String::new();
    let mut inputs = BTreeMap::new();
    for (name, value) in fields {
        if name == "ref" {
            reference = value;
        } else if let Some(input) = name.strip_prefix("inputs[").and_then(|rest| rest.strip_suffix(']')) {
            inputs.insert(input.to_owned(), value);
        }
    }
    let request = DispatchRequest { workflow: workflow.to_owned(), reference: reference.clone(), inputs: inputs.clone() };
    match actions.dispatch(user, &ctx, &request).await {
        Ok(run) => Ok(DispatchResult::Run(run.number)),
        Err(ServiceError::Invalid(error)) => {
            let submitted = Submitted { reference, inputs, error };
            let page = runs_page(state, viewer, page, owner, repo, Some(workflow), &ListQuery::default(), Some(&submitted)).await?;
            Ok(DispatchResult::Invalid(Box::new(page)))
        }
        Err(error) => Err(error.into()),
    }
}

// Actions: disabled workflows.

fn run_number(run: &str) -> WebResult<i64> {
    run.parse().ok().filter(|number| *number > 0).ok_or(WebError::NotFound)
}

// Actions: workflow status badges.
/// The parameters of a workflow's badge, as on GitHub.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BadgeQuery {
    /// A branch; the default branch when empty.
    branch: String,
    /// An event such as `push`; any when empty.
    event: String,
}

/// How long a badge may be reused before it is asked for again.
const BADGE_MAX_AGE: &str = "max-age=60";

/// `GET /{owner}/{repo}/actions/workflows/{workflow}/badge.svg?branch=&event=`: the
/// workflow's status as an SVG badge. Anyone who may read the repository gets it; a private
/// repository's badge answers everyone else 404, exactly like a repository that does not
/// exist. Public badges may be cached by anyone, private ones only by the viewer's browser;
/// the `ETag` is the badge's content, so a revalidation that finds no change costs no body.
pub async fn badge(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, workflow)): Path<(String, String, String)>,
    Query(query): Query<BadgeQuery>,
    headers: axum::http::HeaderMap,
) -> WebResult<axum::response::Response> {
    use axum::{
        http::{HeaderValue, StatusCode, header},
        response::IntoResponse as _,
    };
    let actions = actions(&state)?;
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let branch = Some(query.branch.trim()).filter(|branch| !branch.is_empty());
    let badge = match query.event.trim() {
        "" => actions.badge(&ctx, &workflow, branch, None).await?,
        event => match RunEvent::parse(event) {
            Some(event) => actions.badge(&ctx, &workflow, branch, Some(event)).await?,
            // No run was ever started by an event rbit does not know.
            None => actions
                .badge(&ctx, &workflow, branch, None)
                .await?
                .map(|badge| rbit_services::actions::browse::Badge { passing: None, ..badge }),
        },
    }
    .ok_or(WebError::NotFound)?;
    let svg = crate::views::actions::badge_svg(&badge.label, badge.passing);
    let etag = {
        use sha2::{Digest as _, Sha256};
        let digest = Sha256::digest(svg.as_bytes());
        format!(
            "\"{}\"",
            digest.iter().take(12).fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            })
        )
    };
    let cache = if ctx.repo.repo.visibility.is_public() { format!("public, {BADGE_MAX_AGE}") } else { format!("private, {BADGE_MAX_AGE}") };
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|tags| tags.split(',').any(|tag| tag.trim() == etag || tag.trim() == "*"));
    let mut response = if fresh { StatusCode::NOT_MODIFIED.into_response() } else { svg.into_response() };
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/svg+xml; charset=utf-8"));
    out.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // An SVG opened on its own is a document: it may draw, never run script or load anything.
    out.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"));
    out.insert(header::VARY, HeaderValue::from_static("Cookie"));
    if let Ok(value) = HeaderValue::from_str(&cache) {
        out.insert(header::CACHE_CONTROL, value);
    }
    if let Ok(value) = HeaderValue::from_str(&etag) {
        out.insert(header::ETAG, value);
    }
    Ok(response)
}

/// `GET /{owner}/{repo}/actions/runs/{run}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, run)): Path<(String, String, String)>,
) -> WebResult<RunPage> {
    let actions = actions(&state)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.run_detail(&loaded.ctx, run_number(&run)?).await?;
    let urls = ActionsUrls::new(&loaded.urls.home());
    let run = &detail.run;
    // Writer controls follow the repository's content: none in an archived one.
    let can_write = loaded.ctx.can_write_content();
    let finished = run.status.is_done();
    let has_jobs = !run.is_workflow_error();
    let any_failed = detail.jobs.iter().any(|job| matches!(job.status, Status::Failure | Status::Cancelled));
    // Actions: artifacts (A).
    let artifacts: Vec<_> =
        actions.run_artifacts(&loaded.ctx, run.id).await?.iter().map(|artifact| ArtifactRow::new(artifact, &urls, run.number)).collect();
    // Actions: step summaries.
    let heads = actions.summary_heads(&loaded.ctx, &detail.jobs).await?;
    let mut summaries = Vec::new();
    let mut remaining = 8 * 1024;
    for head in heads.iter().take(4) {
        let Some(job) = detail.jobs.iter().find(|job| job.id == head.job_id) else {
            continue;
        };
        if remaining == 0 {
            break;
        }
        if let Some(text) = actions.summary_preview(&loaded.ctx, job, head.step, remaining).await? {
            let clipped = text.len() >= remaining;
            remaining = remaining.saturating_sub(text.len());
            summaries.push(SummaryView::preview(&job.name, &text, urls.summary(run.number, job.id.0, Some(head.step)), clipped));
        }
    }
    let mut jobs = job_rows(&detail.jobs, run.number, &urls, can_write && finished && has_jobs);
    for job in &mut jobs {
        if let Some(head) = heads.iter().find(|head| head.job_id.0 == job.id) {
            job.summary_url = Some(urls.summary(run.number, job.id, Some(head.step)));
        }
    }
    let jobs_pagination = RunPagination::new(&page, run.number, "jobs_page", jobs.len())?;
    let jobs = jobs.into_iter().skip(jobs_pagination.offset).take(32).collect();
    let artifacts_pagination = RunPagination::new(&page, run.number, "artifacts_page", artifacts.len())?;
    let artifacts = artifacts.into_iter().skip(artifacts_pagination.offset).take(32).collect();
    let self_url = crate::views::actions::run_page_url(&page, run.number, None)?;
    // Actions: environments.
    let deployments = actions
        .pending_deployments(viewer.user(), &loaded.ctx, run)
        .await?
        .into_iter()
        .map(|pending| DeploymentView {
            environment: pending.environment.name,
            jobs: pending.jobs.iter().map(|job| job.name.as_str()).collect::<Vec<_>>().join(", "),
            // Reviewers need only read access, but nothing changes in an archived repository.
            can_review: pending.can_review && !loaded.ctx.repo.repo.is_archived(),
        })
        .collect();
    Ok(RunPage {
        jobs_pagination,
        artifacts_pagination,
        page: page.titled(format!("{} #{} · Actions · {}", run.workflow_name, run.number, loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "actions"),
        run: RunSummary::new(run, &detail.people, &urls, loaded.urls.commit(&run.sha)),
        jobs,
        active: !finished,
        self_url,
        runs_url: urls.runs(),
        can_write,
        awaiting_approval: run.status == Status::AwaitingApproval,
        cancel_url: (!finished).then(|| urls.run_action(run.number, "cancel")),
        rerun_url: (finished && has_jobs).then(|| urls.run_action(run.number, "rerun")),
        rerun_failed_url: (finished && has_jobs && any_failed).then(|| urls.run_action(run.number, "rerun-failed")),
        approve_url: urls.run_action(run.number, "approve"),
        artifacts,
        summaries,
        deployments,
        review_url: urls.run_action(run.number, "review"),
    })
}

// Actions: environments.

/// Loads the repository and run `#run` for a signed-in viewer; the service checks write
/// access for each action.
pub(crate) async fn load_run(
    state: &AppState,
    viewer: &Viewer,
    owner: &str,
    repo: &str,
    run: &str,
) -> WebResult<(RepoPage, RunId, String)> {
    let actions = actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    let detail = actions.run_detail(&loaded.ctx, run_number(run)?).await?;
    let url = ActionsUrls::new(&loaded.urls.home()).run(detail.run.number);
    Ok((loaded, detail.run.id, url))
}

// Actions: artifacts (A).
/// `GET /{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}`: the artifact's tar.gz.
pub async fn artifact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, run, artifact)): Path<(String, String, String, String)>,
) -> WebResult<axum::response::Response> {
    download(&state, &viewer, (&owner, &repo, &run, &artifact), super::ArtifactFormat::TarGz).await
}

/// `GET /{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}/{format}`: the artifact as a
/// `zip` or a `tar.gz`.
pub async fn artifact_as(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, run, artifact, format)): Path<(String, String, String, String, String)>,
) -> WebResult<axum::response::Response> {
    let format = super::ArtifactFormat::parse(&format).ok_or(WebError::NotFound)?;
    download(&state, &viewer, (&owner, &repo, &run, &artifact), format).await
}

async fn download(
    state: &AppState,
    viewer: &Viewer,
    (owner, repo, run, artifact): (&str, &str, &str, &str),
    format: super::ArtifactFormat,
) -> WebResult<axum::response::Response> {
    let actions = actions(state)?;
    let ctx = state.services.repositories.open(viewer.user(), owner, repo).await?;
    let detail = actions.run_detail(&ctx, run_number(run)?).await?;
    let id = rbit_core::actions::ArtifactId(artifact.parse().map_err(|_| WebError::NotFound)?);
    let (artifact, file) = actions.open_artifact(&ctx, id).await?;
    if artifact.run_id != detail.run.id {
        return Err(WebError::NotFound);
    }
    Ok(super::artifact_download(&actions, &artifact, file, format))
}

/// One full current-task step summary. Neighbor identities come from the bounded
/// domain accessor rather than loading all bodies into the run document.
pub(crate) async fn summary_page(
    state: &AppState,
    viewer: &Viewer,
    mut page: Page,
    owner: &str,
    repo: &str,
    selection: (i64, rbit_core::actions::JobId, Option<i64>),
) -> WebResult<crate::views::actions::SummaryPage> {
    let (run, job, step) = selection;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    let actions = actions(state)?;
    let detail = actions.job_detail(&loaded.ctx, job).await?;
    if detail.run.number != run {
        return Err(WebError::NotFound);
    }
    let summary = actions.step_summary(&loaded.ctx, &detail.job, step).await?.ok_or(WebError::NotFound)?;
    let urls = ActionsUrls::new(&loaded.urls.home());
    page.path = urls.summary(run, job.0, Some(summary.number));
    let previous = summary.previous.map(|step| urls.summary(run, job.0, Some(step)));
    let next = summary.next.map(|step| urls.summary(run, job.0, Some(step)));
    let html = crate::render::render_markdown_bounded(&summary.text, 512 * 1024);
    let title = page.locale.format(&rbit_messages::MessageRef {
        namespace: "actions",
        key: "summary-title",
        args: &[rbit_messages::Argument { name: "job", value: rbit_messages::Value::Text(&detail.job.name) }],
    });
    Ok(crate::views::actions::SummaryPage {
        page: page.titled(title),
        header: loaded.header(state, "actions"),
        run_url: urls.run(run),
        job: detail.job.name,
        number: summary.number,
        html,
        source: summary.text,
        previous,
        next,
    })
}
