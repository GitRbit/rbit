//! LFS objects in the repository browser: the blob page shows the object a pointer file
//! names (sized, rendered and downloadable like any file) and `raw` serves its bytes.
//! Without the object (never uploaded, or `[lfs]` disabled) the pointer itself is shown.

use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use rbit_core::{User, lfs::LfsPointer};
use rbit_git::{BlobView, diff::is_binary};
use rbit_services::RepoContext;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

use crate::AppState;

/// How much of an object decides whether `raw` serves it as text (Git's own window).
const SNIFF_BYTES: usize = 8000;

/// A pointer file found on the blob page.
#[derive(Debug)]
pub(in crate::routes) struct LfsBlob {
    /// Whether the server has the object; the page then shows it instead of the pointer.
    pub(in crate::routes) stored: bool,
}

/// When `view` is a pointer file: whether the object is stored and, if so, `view` with
/// the object's size and (up to `max_bytes`) content in place of the pointer's.
pub(in crate::routes) async fn lfs_blob(
    state: &AppState,
    ctx: &RepoContext,
    viewer: Option<&User>,
    view: BlobView,
    max_bytes: usize,
) -> (BlobView, Option<LfsBlob>) {
    let Some(pointer) = view.data.as_deref().and_then(LfsPointer::parse) else { return (view, None) };
    let lfs = &state.services.lfs;
    if !lfs.enabled() {
        return (view, None);
    }
    let mut download = match lfs.open(ctx, viewer, &pointer.oid).await {
        Ok(download) => download,
        Err(error) => {
            if !matches!(error, rbit_services::ServiceError::NotFound) {
                tracing::warn!(%error, repo = %ctx.repo.full_name(), oid = %pointer.oid, "cannot open an LFS object");
            }
            return (view, Some(LfsBlob { stored: false }));
        }
    };
    let data = if usize::try_from(download.size).is_ok_and(|size| size <= max_bytes) {
        let mut data = Vec::with_capacity(usize::try_from(download.size).unwrap_or_default());
        if let Err(error) = download.file.read_to_end(&mut data).await {
            tracing::warn!(%error, oid = %pointer.oid, "cannot read an LFS object");
            return (view, Some(LfsBlob { stored: false }));
        }
        Some(data)
    } else {
        None
    };
    let is_binary = data.as_deref().is_some_and(is_binary);
    (BlobView { commit: view.commit, size: download.size, data, is_binary }, Some(LfsBlob { stored: true }))
}

/// `raw` for a pointer file whose object is stored: the object's bytes, streamed, typed
/// like any raw file (`image_type`, else text or `application/octet-stream` by content).
/// `None` serves the pointer itself.
pub(in crate::routes) async fn lfs_raw(
    state: &AppState,
    ctx: &RepoContext,
    viewer: Option<&User>,
    view: &BlobView,
    image_type: Option<&'static str>,
) -> Option<Response> {
    let pointer = view.data.as_deref().and_then(LfsPointer::parse)?;
    let lfs = &state.services.lfs;
    if !lfs.enabled() {
        return None;
    }
    let mut download = lfs.open(ctx, viewer, &pointer.oid).await.ok()?;
    let mut prefix = Vec::with_capacity(SNIFF_BYTES);
    if let Err(error) = (&mut download.file).take(SNIFF_BYTES as u64).read_to_end(&mut prefix).await {
        tracing::warn!(%error, oid = %pointer.oid, "cannot read an LFS object");
        return None;
    }
    let content_type = match image_type {
        Some(image) => image,
        None if is_binary(&prefix) => "application/octet-stream",
        None => "text/plain; charset=utf-8",
    };
    let body = std::io::Cursor::new(prefix).chain(download.file);
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
        (header::CONTENT_LENGTH, HeaderValue::from(download.size)),
        (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'")),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
    ];
    Some((headers, Body::from_stream(ReaderStream::new(body))).into_response())
}
