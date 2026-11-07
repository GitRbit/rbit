//! Overview document adapters: existing queries with locale-aware document headers.
use super::{ListQuery, runs_page};
use crate::{AppState, error::WebResult, extract::Viewer, localization::RequestLocale, views::layout::Page};
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};

/// `GET /{owner}/{repo}/actions`
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<Response> {
    let view = runs_page(&state, &viewer, page.with_locale(locale.0.locale), &owner, &repo, None, &query, None).await?;
    Ok(locale.response(view.into_response()))
}

/// `GET /{owner}/{repo}/actions/workflows/{workflow}`: the runs of one workflow file.
pub async fn workflow(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, workflow)): Path<(String, String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<Response> {
    let view = runs_page(&state, &viewer, page.with_locale(locale.0.locale), &owner, &repo, Some(&workflow), &query, None).await?;
    Ok(locale.response(view.into_response()))
}
