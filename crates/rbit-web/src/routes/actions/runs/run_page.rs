//! One authorized run document, with locale-aware presentation and unchanged service reads.
use super::{actions, run_number};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    localization::RequestLocale,
    routes::repo::RepoPage,
    views::{
        actions::{
            ActionsUrls, AnnotationsView, ArtifactRow, DeploymentView, JobGraph, RunPage, RunPageLive, RunSidebar, RunSummary, SummaryView,
            TriggerView, job_rows, name_runners,
        },
        layout::Page,
    },
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use rbit_core::actions::Status;
use rbit_localization::Direction;

/// `GET /{owner}/{repo}/actions/runs/{run}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    headers: HeaderMap,
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
    let artifacts: Vec<ArtifactRow> =
        actions.run_artifacts(&loaded.ctx, run.id).await?.iter().map(|artifact| ArtifactRow::new(artifact, &urls, run.number)).collect();
    // Actions: step summaries.
    let summaries = actions
        .job_summaries(&loaded.ctx, &detail.jobs)
        .await?
        .iter()
        .filter_map(|(id, texts)| detail.jobs.iter().find(|job| job.id == *id).map(|job| SummaryView::new(&job.name, texts)))
        .collect();
    // Actions: annotations.
    let annotations = AnnotationsView::new(&actions.run_annotations(&loaded.ctx, &detail.jobs).await?, &detail.jobs, run.number, &urls);
    // Actions: run page. How the run started, and who ran its jobs where.
    let trigger_details = actions.trigger_details(&loaded.ctx, run).await?;
    let mut jobs = job_rows(&detail.jobs, run.number, &urls, can_write && finished && has_jobs);
    name_runners(&mut jobs, &actions.runner_names(&detail.jobs).await?);
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
        trigger: TriggerView::new(run, &detail.people, trigger_details, &loaded.urls.home(), artifacts.len()),
        jobs,
        annotations,
        graph: JobGraph::new(&detail.jobs, run.number, &urls)
            .map(|graph| if locale.0.locale.direction() == Direction::RightToLeft { graph.mirrored() } else { graph }),
        sidebar: RunSidebar::new(run, &detail.jobs, &urls, loaded.urls.blob(&run.sha, &run.workflow_path), None, locale.0.locale),
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
    // The run section's own poll gets that section and the job list, not the whole page.
    if polling(&headers) {
        return Ok(locale.response(RunPageLive(view).into_response()));
    }
    Ok(locale.response(view.into_response()))
}

/// A request from the run section's poll: htmx targets `#run` and it is no navigation.
fn polling(headers: &HeaderMap) -> bool {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    header("hx-request") == Some("true") && header("hx-target") == Some("run") && header("hx-boosted").is_none()
}
