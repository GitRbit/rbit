//! Authenticated repository reads for the paged branch/tag picker and its fragment.

use super::repo::{RepoPage, open_pinned};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        layout::Page,
        ref_picker::{RefPicker, RefPickerOptions, RefPickerPage, RefPickerQuery},
    },
};
use axum::extract::{Path, Query, State};

pub async fn page(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<RefPickerQuery>,
) -> WebResult<RefPickerPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    Ok(RefPickerPage {
        page: page.titled(format!("Branches and tags · {owner}/{repo}")),
        header: loaded.header(&state, "code"),
        picker: RefPicker::new(loaded.refs(), loaded.default_branch(), &loaded.urls, query),
    })
}

pub async fn options(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<RefPickerQuery>,
) -> WebResult<RefPickerOptions> {
    let ctx = open_pinned(&state, &viewer, &owner, &repo).await?;
    let refs = ctx.reader.refs().await.map_err(rbit_services::ServiceError::from)?;
    let urls = crate::urls::RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
    Ok(RefPickerOptions { picker: RefPicker::new(&refs, &ctx.repo.repo.default_branch, &urls, query) })
}
