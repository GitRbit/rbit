//! One authorized run document, with locale-aware presentation and unchanged service reads.
use super::{actions, run_number};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    localization::RequestLocale,
    routes::repo::RepoPage,
    views::{
        actions::{ActionsUrls, ArtifactRow, DeploymentView, RunPage, RunSummary, SummaryView, job_rows},
        layout::Page,
    },
};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use rbit_core::actions::Status;

/// `GET /{owner}/{repo}/actions/runs/{run}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, run)): Path<(String, String, String)>,
) -> WebResult<Response> {
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
    // Actions: environments.
    let deployments = actions
        .pending_deployments(viewer.user(), &loaded.ctx, run)
        .await?
        .into_iter()
        .map(|pending| DeploymentView {
            environment: pending.environment.name,
            jobs: pending.jobs.iter().map(|job| job.name.as_str()).collect::<Vec<_>>().join(", "),
            job_count: u64::try_from(pending.jobs.len()).unwrap_or(u64::MAX),
            // Reviewers need only read access, but nothing changes in an archived repository.
            can_review: pending.can_review && !loaded.ctx.repo.repo.is_archived(),
        })
        .collect();
    let view = RunPage {
        page: page.with_locale(locale.0.locale).titled(format!(
            "{} #{} · Actions · {}",
            run.workflow_name,
            run.number,
            loaded.ctx.repo.full_name()
        )),
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
        deployments,
        review_url: urls.run_action(run.number, "review"),
    };
    Ok(locale.response(view.into_response()))
}
