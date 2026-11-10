//! The run's job list alone, for a job page's navigation to refresh itself while the run
//! is not finished. Readers only, like the run page.
use super::{actions, run_number};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    localization::RequestLocale,
    routes::repo::RepoPage,
    views::actions::{ActionsUrls, RunSidebar, RunSidebarJobs},
};
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};
use rbit_core::actions::JobId;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SidebarQuery {
    /// The job the page shows.
    job: Option<i64>,
}

/// `GET /{owner}/{repo}/actions/runs/{run}/sidebar?job={id}`
pub async fn sidebar(
    State(state): State<AppState>,
    viewer: Viewer,
    locale: RequestLocale,
    Path((owner, repo, run)): Path<(String, String, String)>,
    Query(query): Query<SidebarQuery>,
) -> WebResult<Response> {
    let actions = actions(&state)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = actions.run_detail(&loaded.ctx, run_number(&run)?).await?;
    let (run, urls) = (&detail.run, ActionsUrls::new(&loaded.urls.home()));
    let current = query.job.map(JobId).filter(|id| detail.jobs.iter().any(|job| job.id == *id));
    let url = match current {
        Some(job) => format!("{}/sidebar?job={}", urls.run(run.number), job.0),
        None => format!("{}/sidebar", urls.run(run.number)),
    };
    let workflow_file = loaded.urls.blob(&run.sha, &run.workflow_path);
    let sidebar = RunSidebar::new(run, &detail.jobs, &urls, workflow_file, current, locale.0.locale).polling(run, url);
    Ok(locale.response(RunSidebarJobs { sidebar }.into_response()))
}
