//! Source archives: `/{owner}/{repo}/archive/{revision}.zip` and `.tar.gz`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged outside the page router (archives stream for longer than the page timeout and
//! are already compressed), so this router applies the session, error-page and
//! security-header middleware itself. Access is exactly that of browsing: the viewer's
//! session decides, and a repository they cannot read is a 404.
//!
//! A download stops when the client reads nothing (or git writes nothing) for
//! `[repository] archive_idle_timeout_seconds`, or after `archive_max_seconds`, so slow
//! readers cannot keep the few archive slots.

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::from_fn_with_state,
    response::{IntoResponse, Response},
    routing::get,
};
use rbit_git::archive::ArchiveFormat;
use rbit_services::archives::{ArchiveError, BUSY_RETRY_SECONDS};

use crate::{
    AppState,
    client_ip::ClientIp,
    error::{WebError, WebResult},
    extract::Viewer,
    middleware,
    supervise::{Activity, StreamLimits, supervised_body},
};

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/archive/{*rest}", get(download))
        .layer(from_fn_with_state(state.clone(), middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), middleware::session))
        .layer(from_fn_with_state(middleware::SecurityHeaders::new(&state.config), middleware::security_headers))
}

/// `GET /{owner}/{repo}/archive/{revision}.zip|.tar.gz` (the revision may contain `/`).
async fn download(
    State(state): State<AppState>,
    viewer: Viewer,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let (revision, format) = ArchiveFormat::split_suffix(&rest).ok_or(WebError::NotFound)?;
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let archive = match state.services.archives.archive(&ctx, revision, format, ip).await {
        Ok(archive) => archive,
        Err(ArchiveError::Busy) => {
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, BUSY_RETRY_SECONDS.to_string())],
                "Too many archives are being prepared right now. Try again in a few seconds.",
            )
                .into_response());
        }
        Err(ArchiveError::TooManyFromClient) => {
            return Ok((
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, BUSY_RETRY_SECONDS.to_string())],
                "You are already downloading as many archives as allowed. Try again when one has finished.",
            )
                .into_response());
        }
        Err(ArchiveError::Service(error)) => return Err(error.into()),
    };

    let etag = format!("\"{}\"", archive.commit_id);
    let cache = [(header::ETAG, etag.clone()), (header::CACHE_CONTROL, "private, max-age=0, must-revalidate".to_owned())];
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(|tag| tag.trim() == etag)) {
        return Ok((StatusCode::NOT_MODIFIED, cache).into_response());
    }

    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", archive.file_name)).map_err(WebError::internal)?;
    let limits =
        StreamLimits { idle: state.config.repository.archive_idle_timeout(), total: state.config.repository.archive_max_duration() };
    Ok((
        cache,
        [(header::CONTENT_TYPE, HeaderValue::from_static(format.content_type())), (header::CONTENT_DISPOSITION, disposition)],
        // The body holds the archive slot; the supervisor drops it once the download stalls.
        supervised_body(archive.body, (), Activity::start(limits)),
    )
        .into_response())
}
