//! "Go to file" and the commit count above the file table.
//!
//! Both are pinned to a commit id by the tree page, so their answers never change at
//! their URL and public repositories let browsers and caches keep them.

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use rbit_services::ServiceError;
use serde::Deserialize;

use super::{RepoPage, open_pinned};
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    localization::RequestLocale,
    urls::{RepoUrls, encode_path},
    views::{
        file_finder::{CommitCountFragment, FileFinder, FindPage, FoundFile, file_match_score},
        home::group_thousands,
        layout::Page,
    },
};

/// Commits counted before the count is shown as "N+" instead.
const COMMIT_COUNT_LIMIT: u64 = 1_000_000;
/// Files the finder lists for one revision.
const FINDER_FILE_LIMIT: usize = 100_000;
/// Matches shown on the find page.
const FIND_RESULTS: usize = 100;

pub(super) fn commit_count_url(urls: &RepoUrls, commit_id: &str) -> String {
    format!("{}/commit-count/{commit_id}", urls.home())
}

/// The finder for the tree page of `revision`, whose paths are those of `commit_id`.
pub(super) fn finder(urls: &RepoUrls, revision: &str, commit_id: &str) -> FileFinder {
    let action = format!("{}/find/{}", urls.home(), encode_path(revision));
    FileFinder {
        paths_url: format!("{}/find/{commit_id}?format=paths", urls.home()),
        blob_base: format!("{}/", urls.blob(revision, "").trim_end_matches('/')),
        query: String::new(),
        action,
    }
}

/// `GET /{owner}/{repo}/commit-count/{revision}`
pub async fn commit_count(
    State(state): State<AppState>,
    viewer: Viewer,
    locale: RequestLocale,
    Path((owner, repo, revision)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let ctx = open_pinned(&state, &viewer, &owner, &repo).await?;
    let commit_id = ctx.reader.commit_id(revision.clone()).await.map_err(ServiceError::from)?;
    let (count, capped) = ctx.reader.commit_count(commit_id.clone(), COMMIT_COUNT_LIMIT).await.map_err(ServiceError::from)?;
    let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
    let fragment = CommitCountFragment {
        locale: locale.document_locale(),
        url: urls.commits(&revision),
        raw_count: usize::try_from(count).unwrap_or(usize::MAX),
        count: group_thousands(i64::try_from(count).unwrap_or(i64::MAX)),
        capped,
        singular: count == 1,
    };
    let pinned = commit_id == revision && ctx.repo.repo.visibility.is_public();
    Ok(crate::middleware::cache_pinned(locale.response(fragment.into_response()), pinned))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FindQuery {
    q: String,
    /// `paths`: every path, one per line, for the in-page finder.
    format: String,
}

/// `GET /{owner}/{repo}/find/{revision}`: the files of a revision matching `q`, or with
/// `format=paths` all of them as plain text.
pub async fn find(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Query(query): Query<FindQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, _) = loaded.split(rest).await?;
    let reader = &loaded.ctx.reader;
    let commit_id = reader.commit_id(revision.clone()).await.map_err(ServiceError::from)?;
    let (paths, truncated) = reader.file_paths(commit_id.clone(), FINDER_FILE_LIMIT).await.map_err(ServiceError::from)?;

    if query.format == "paths" {
        let mut body = String::with_capacity(paths.iter().map(|path| path.len() + 1).sum());
        for path in paths.iter().filter(|path| !path.contains(['\n', '\r'])) {
            body.push_str(path);
            body.push('\n');
        }
        let response = ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response();
        let pinned = commit_id == revision && loaded.ctx.repo.repo.visibility.is_public();
        return Ok(crate::middleware::cache_pinned(response, pinned));
    }

    let urls = &loaded.urls;
    let mut finder = finder(urls, &revision, &commit_id);
    let mut matches: Vec<(i64, &String)> =
        paths.iter().filter_map(|path| file_match_score(path, &query.q).map(|score| (score, path))).collect();
    if !query.q.trim().is_empty() {
        matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    }
    let more = matches.len().saturating_sub(FIND_RESULTS);
    let results = matches
        .into_iter()
        .take(FIND_RESULTS)
        .map(|(_, path)| FoundFile { url: format!("{}{}", finder.blob_base, encode_path(path)), path: path.clone() })
        .collect();
    let title = format!("{} · {}", rbit_localization::RepoBrowseLabel::GoToFile.text(&page.locale), loaded.ctx.repo.full_name());
    finder.query = query.q;
    Ok(FindPage {
        page: page.titled(title),
        header: loaded.header_at(&state, "code", &revision),
        tree_url: urls.tree(&revision, ""),
        revision,
        finder,
        results,
        more,
        truncated,
    }
    .into_response())
}
