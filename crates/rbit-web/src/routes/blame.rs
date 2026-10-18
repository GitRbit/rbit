//! Blame view: `/{owner}/{repo}/blame/{revision}/{path}`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use rbit_git::blame::Blame;
use rbit_services::ServiceError;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    views::{
        blame::{BlameContent, BlameHunkView, BlamePage},
        layout::Page,
        repo::{CommitView, breadcrumbs},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/{owner}/{repo}/blame/{*rest}", get(show))
}

/// `GET /{owner}/{repo}/blame/{revision}/{path…}`
async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<BlamePage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = loaded.split(rest).await?;
    if path.is_empty() {
        return Err(WebError::NotFound);
    }
    let urls = &loaded.urls;
    let file_name = path.rsplit_once('/').map_or(path.as_str(), |(_, name)| name).to_owned();

    let resolved = state.services.blame.blame(&loaded.ctx, &revision, &path).await?;
    let blame = &resolved.result;
    // The banner shows the revision's commit even when the file is not blamed.
    let latest = match blame.as_ref() {
        Blame::Blamed(blamed) => blamed.commit.clone(),
        Blame::Binary | Blame::TooLarge => {
            loaded.ctx.reader.blob(resolved.commit_id, path.clone(), 0).await.map_err(ServiceError::from)?.commit
        }
    };
    let content = match blame.as_ref() {
        Blame::Blamed(blamed) => {
            let highlighter = state.clone();
            let name = file_name.clone();
            let shared = std::sync::Arc::clone(blame);
            let lines = tokio::task::spawn_blocking(move || match shared.as_ref() {
                Blame::Blamed(file) => highlighter.highlighter.highlight_lines_cached(&file.blob_id, &name, &file.text),
                _ => Vec::new(),
            })
            .await
            .map_err(WebError::internal)?;
            BlameContent::Hunks(BlameHunkView::build(&blamed.hunks, &lines, urls))
        }
        Blame::Binary => BlameContent::Binary,
        Blame::TooLarge => BlameContent::TooLarge,
    };

    Ok(BlamePage {
        page: page.titled(format!("Blame {path} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "code"),
        switcher: loaded.switcher(&revision, |r| urls.blame(r, &path)),
        breadcrumbs: breadcrumbs(loaded.ctx.repo.repo.name.as_str(), urls, &revision, &path),
        latest: CommitView::new(&latest, urls),
        file_name,
        blob_url: urls.blob(&revision, &path),
        raw_url: urls.raw(&revision, &path),
        content,
    })
}
