//! The runs list, a run page and what writers do to runs: cancel, re-run (all, failed, one
//! job) and approve; and on a workflow's page, running it by hand (`workflow_dispatch`).
//!
//! Every page needs a reader of a repository with Actions on (`ActionsService::list_runs`
//! and `run_detail` answer 404 otherwise); the buttons post to handlers the service checks
//! for write access again.
//!
//! Workstream U (interface and secrets) owns this file.

use axum::{
    Form,
    extract::{Path, Query, State},
    response::Redirect,
};
use std::collections::BTreeMap;

use rbit_core::actions::{JobId, RunEvent, RunId, Status};
use rbit_git::RefKind;
use rbit_services::{
    ActionsService, ServiceError,
    actions::{browse::RunQuery, dispatch::DispatchRequest, runs::Rerun},
};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    routes::repo::RepoPage,
    views::{
        actions::{
            ActionsUrls, ArtifactRow, DispatchFormView, DispatchInputView, RunFilters, RunPage, RunRow, RunSummary, RunsPage, SummaryView,
            WorkflowLink, job_rows, workflow_file,
        },
        layout::Page,
    },
};

fn actions(state: &AppState) -> WebResult<&ActionsService> {
    state.services.actions.as_ref().ok_or(WebError::NotFound)
}

/// The filters as GET parameters; empty values mean "any".
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    branch: String,
    actor: String,
    event: String,
    status: String,
    before: Option<i64>,
    // Actions: dispatch form.
    /// The branch or tag the dispatch form reads the workflow at.
    #[serde(rename = "ref")]
    reference: String,
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

/// `GET /{owner}/{repo}/actions`
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<RunsPage> {
    runs_page(&state, &viewer, page, &owner, &repo, None, &query, None).await
}

/// `GET /{owner}/{repo}/actions/workflows/{workflow}`: the runs of one workflow file.
pub async fn workflow(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, workflow)): Path<(String, String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<RunsPage> {
    runs_page(&state, &viewer, page, &owner, &repo, Some(&workflow), &query, None).await
}

// Actions: dispatch form.
/// A dispatch that failed, shown again with its values and why.
struct Submitted {
    reference: String,
    inputs: BTreeMap<String, String>,
    error: String,
}

#[allow(clippy::too_many_arguments)]
async fn runs_page(
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
    let dispatch = match &selected {
        Some((path, _)) if loaded.ctx.can_write_content() => dispatch_form(actions, &loaded, &urls, path, query, submitted).await?,
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
    let next_url = listing.next.map(|before| {
        let filters = query.encoded();
        format!("{action}?{}before={}", if filters.is_empty() { String::new() } else { format!("{filters}&") }, before.0)
    });
    let workflows = known
        .iter()
        .map(|(path, name)| WorkflowLink {
            name: name.clone(),
            file: path.clone(),
            url: urls.workflow(path),
            is_current: selected.as_ref().is_some_and(|(selected, _)| selected == path),
        })
        .collect();
    let title = match &selected {
        Some((_, name)) => format!("{name} · Actions · {}", loaded.ctx.repo.full_name()),
        None => format!("Actions · {}", loaded.ctx.repo.full_name()),
    };
    Ok(RunsPage {
        page: page.titled(title),
        header: loaded.header(state, "actions"),
        runs: listing.runs.iter().map(|run| RunRow::new(run, &listing.people, &urls)).collect(),
        workflows,
        all_url: urls.runs(),
        workflow: selected.map(|(_, name)| name),
        filters: RunFilters::new(action, query.branch.trim(), query.actor.trim(), query.event(), query.status()),
        next_url,
        dispatch,
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
        action: format!("{}/dispatch", urls.workflow(path)),
        page_url: urls.workflow(path),
        reference: shown,
        refs,
        inputs,
        open: error.is_some() || !query.reference.trim().is_empty(),
        error,
        can_run: form.is_some(),
    }))
}

/// `POST /{owner}/{repo}/actions/workflows/{workflow}/dispatch`: runs the workflow at the
/// chosen branch or tag with the inputs given, and shows the new run. What the workflow
/// refuses (a ref, an input) is shown in the form again, with the values given.
pub async fn dispatch(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, workflow)): Path<(String, String, String)>,
    Form(fields): Form<Vec<(String, String)>>,
) -> WebResult<axum::response::Response> {
    use axum::response::IntoResponse as _;
    let actions = actions(&state)?;
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let mut reference = String::new();
    // A boolean is sent as a hidden `false` and, when ticked, a later `true`: the last wins.
    let mut inputs = BTreeMap::new();
    for (name, value) in fields {
        if name == "ref" {
            reference = value;
        } else if let Some(input) = name.strip_prefix("inputs[").and_then(|rest| rest.strip_suffix(']')) {
            inputs.insert(input.to_owned(), value);
        }
    }
    let request = DispatchRequest { workflow: workflow.clone(), reference: reference.clone(), inputs: inputs.clone() };
    match actions.dispatch(&user, &ctx, &request).await {
        Ok(run) => {
            let urls = ActionsUrls::new(&format!("/{}", ctx.repo.full_name()));
            Ok(Redirect::to(&urls.run(run.number)).into_response())
        }
        Err(ServiceError::Invalid(error)) => {
            let submitted = Submitted { reference, inputs, error };
            let page = runs_page(&state, &viewer, page, &owner, &repo, Some(&workflow), &ListQuery::default(), Some(&submitted)).await?;
            Ok((axum::http::StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(error) => Err(error.into()),
    }
}

fn run_number(run: &str) -> WebResult<i64> {
    run.parse().ok().filter(|number| *number > 0).ok_or(WebError::NotFound)
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
    let artifacts =
        actions.run_artifacts(&loaded.ctx, run.id).await?.iter().map(|artifact| ArtifactRow::new(artifact, &urls, run.number)).collect();
    // Actions: step summaries.
    let summaries = actions
        .job_summaries(&loaded.ctx, &detail.jobs)
        .await?
        .iter()
        .filter_map(|(id, texts)| detail.jobs.iter().find(|job| job.id == *id).map(|job| SummaryView::new(&job.name, texts)))
        .collect();
    Ok(RunPage {
        page: page.titled(format!("{} #{} · Actions · {}", run.workflow_name, run.number, loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "actions"),
        run: RunSummary::new(run, &detail.people, &urls, loaded.urls.commit(&run.sha)),
        jobs: job_rows(&detail.jobs, run.number, &urls, can_write && finished && has_jobs),
        active: !finished,
        self_url: urls.run(run.number),
        runs_url: urls.runs(),
        can_write,
        awaiting_approval: run.status == Status::AwaitingApproval,
        cancel_url: (!finished).then(|| urls.run_action(run.number, "cancel")),
        rerun_url: (finished && has_jobs).then(|| urls.run_action(run.number, "rerun")),
        rerun_failed_url: (finished && has_jobs && any_failed).then(|| urls.run_action(run.number, "rerun-failed")),
        approve_url: urls.run_action(run.number, "approve"),
        artifacts,
        summaries,
    })
}

/// `GET /{owner}/{repo}/actions/runs/{run}/jobs`: the jobs are on the run page.
pub async fn jobs(Path((owner, repo, run)): Path<(String, String, String)>) -> WebResult<Redirect> {
    let number = run_number(&run)?;
    Ok(Redirect::to(&ActionsUrls::new(&format!("/{owner}/{repo}")).run(number)))
}

/// Loads the repository and run `#run` for a signed-in viewer; the service checks write
/// access for each action.
async fn load_run(state: &AppState, viewer: &Viewer, owner: &str, repo: &str, run: &str) -> WebResult<(RepoPage, RunId, String)> {
    let actions = actions(state)?;
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    let detail = actions.run_detail(&loaded.ctx, run_number(run)?).await?;
    let url = ActionsUrls::new(&loaded.urls.home()).run(detail.run.number);
    Ok((loaded, detail.run.id, url))
}

/// `POST /{owner}/{repo}/actions/runs/{run}/cancel`
pub async fn cancel(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, run)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let (loaded, id, url) = load_run(&state, &viewer, &owner, &repo, &run).await?;
    actions(&state)?.cancel(&user, &loaded.ctx, id).await?;
    Ok(Redirect::to(&url))
}

async fn rerun(
    state: &AppState,
    user: &rbit_core::User,
    viewer: &Viewer,
    path: (String, String, String),
    which: impl FnOnce() -> Rerun,
) -> WebResult<Redirect> {
    let (owner, repo, run) = path;
    let (loaded, id, url) = load_run(state, viewer, &owner, &repo, &run).await?;
    actions(state)?.rerun(user, &loaded.ctx, id, which()).await?;
    Ok(Redirect::to(&url))
}

/// `POST /{owner}/{repo}/actions/runs/{run}/rerun`
pub async fn rerun_all(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path(path): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    rerun(&state, &user, &viewer, path, || Rerun::All).await
}

/// `POST /{owner}/{repo}/actions/runs/{run}/rerun-failed`
pub async fn rerun_failed(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path(path): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    rerun(&state, &user, &viewer, path, || Rerun::Failed).await
}

/// `POST /{owner}/{repo}/actions/runs/{run}/jobs/{job}/rerun`
pub async fn rerun_job(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, run, job)): Path<(String, String, String, String)>,
) -> WebResult<Redirect> {
    let job = JobId(job.parse().map_err(|_| WebError::NotFound)?);
    rerun(&state, &user, &viewer, (owner, repo, run), || Rerun::Job(job)).await
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ApproveForm {
    trust_author: Option<String>,
}

/// `POST /{owner}/{repo}/actions/runs/{run}/approve`
pub async fn approve(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, run)): Path<(String, String, String)>,
    Form(form): Form<ApproveForm>,
) -> WebResult<Redirect> {
    let (loaded, id, url) = load_run(&state, &viewer, &owner, &repo, &run).await?;
    let trust_author = form.trust_author.as_deref() == Some("true");
    actions(&state)?.approve(&user, &loaded.ctx, id, trust_author).await?;
    Ok(Redirect::to(&url))
}

// Actions: artifacts (A).
/// `GET /{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}`: the artifact's tar.gz.
pub async fn artifact(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, run, artifact)): Path<(String, String, String, String)>,
) -> WebResult<axum::response::Response> {
    use axum::{
        http::{HeaderValue, header},
        response::IntoResponse as _,
    };
    let actions = actions(&state)?;
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let detail = actions.run_detail(&ctx, run_number(&run)?).await?;
    let id = rbit_core::actions::ArtifactId(artifact.parse().map_err(|_| WebError::NotFound)?);
    let (artifact, file) = actions.open_artifact(&ctx, id).await?;
    if artifact.run_id != detail.run.id {
        return Err(WebError::NotFound);
    }
    let mut response = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/gzip"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    if let Ok(value) = HeaderValue::from_str(&artifact.size.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    // Names are [A-Za-z0-9._-], so they need no quoting beyond the quotes.
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{}.tar.gz\"", artifact.name)) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}
