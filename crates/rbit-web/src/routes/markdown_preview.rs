//! Bounded, read-only Markdown previews using the same renderers as saved content.

use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::post,
};
use rbit_core::{IssueText, file_edit::FilePath, issue::MAX_TEXT_BYTES};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    render::{
        references::{RefContext, render_issue_markdown},
        render_markdown,
    },
    urls::{DocumentBase, RepoUrls},
};

pub fn routes(state: &AppState) -> Router<AppState> {
    // Form encoding can expand each byte sixfold (including line endings).
    let limit = state.services.edits.max_text_bytes().max(MAX_TEXT_BYTES).saturating_mul(6).saturating_add(4096);
    Router::new().route("/issues/markdown-preview", post(preview)).layer(DefaultBodyLimit::max(limit))
}

#[derive(Debug, Deserialize)]
pub struct Context {
    owner: String,
    repo: String,
    /// Present only for the file editor, where references stay ordinary Markdown.
    revision: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Preview {
    body: String,
    content: String,
    path: String,
}

/// The page router supplies Origin checking, request throttling and its body bound.
async fn preview(
    State(state): State<AppState>,
    viewer: Viewer,
    Query(context): Query<Context>,
    Form(form): Form<Preview>,
) -> WebResult<Response> {
    let repo = state.services.repositories.open(viewer.user(), &context.owner, &context.repo).await?;
    let urls = RepoUrls::new(repo.repo.owner.name.as_str(), repo.repo.repo.name.as_str());
    let html = if let Some(revision) = context.revision {
        if form.content.len() > state.services.edits.max_text_bytes() || revision.len() > 1024 {
            return Err(WebError::BadRequest("Markdown preview is too large".to_owned()));
        }
        let path = FilePath::parse(&form.path).map_err(|error| WebError::BadRequest(error.to_string()))?;
        render_markdown(&form.content, Some(&DocumentBase::new(urls, revision, path.parent())))
    } else {
        let body = IssueText::body(&form.body).map_err(|error| WebError::BadRequest(error.to_string()))?;
        let refs = state.services.references.resolve(&repo, &[body.as_str()]).await?;
        render_issue_markdown(
            body.as_str(),
            &RefContext {
                base: DocumentBase::new(urls, repo.repo.repo.default_branch.clone(), ""),
                owner: repo.repo.owner.name.to_string(),
                repo: repo.repo.repo.name.to_string(),
                refs,
            },
        )
    };
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Html(if html.trim().is_empty() { "<p class=\"text-ink-muted\">Nothing to preview.</p>".to_owned() } else { html }),
    )
        .into_response())
}
