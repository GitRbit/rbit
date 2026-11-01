//! rbit Actions pages: the repository's runs, a run, a job and its logs, the dispatch form,
//! and the settings for runners, secrets and variables at repository, owner and instance
//! level.
//!
//! [`routes`] is merged into the page router only when `Services::actions` exists, so with
//! Actions disabled every path below answers 404 like any unknown page. [`PATHS`] lists every
//! path template this module serves, with its methods; the disabled-is-disabled tests walk
//! it.
//!
//! The Actions foundation (workstream F) owns the route list; workstream U (interface and
//! secrets) owns the handlers.

pub(crate) mod environments;
pub(crate) mod jobs;
pub(crate) mod runners;
pub(crate) mod runs;
pub(crate) mod settings;

use axum::{Router, http::Method, routing::get};

use crate::AppState;

// Actions: artifacts (A).
/// The formats an artifact downloads in: a zip, which is what GitHub serves, or a tar.gz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactFormat {
    TarGz,
    Zip,
}

impl ArtifactFormat {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "tar.gz" => Some(Self::TarGz),
            "zip" => Some(Self::Zip),
            _ => None,
        }
    }
}

/// The download of `artifact`, whose stored file is `file`, in `format`. The archive is
/// written as it is sent ([`rbit_services::actions::downloads`]), so it has no
/// `Content-Length`; a broken stored archive ends the response with an error rather than
/// finishing an archive that would look whole.
pub(crate) fn artifact_download(
    actions: &rbit_services::ActionsService,
    artifact: &rbit_core::actions::Artifact,
    file: tokio::fs::File,
    format: ArtifactFormat,
) -> axum::response::Response {
    use axum::{
        body::{Body, Bytes},
        http::{HeaderValue, header},
        response::IntoResponse as _,
    };
    use rbit_services::actions::downloads;
    let Ok(lease) = actions.admit() else { return crate::error::WebError::NotFound.into_response() };
    let cancellation = actions.cancellation_token();
    let (kind, extension) = match format {
        ArtifactFormat::TarGz => ("application/gzip", "tar.gz"),
        ArtifactFormat::Zip => ("application/zip", "zip"),
    };
    let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(8);
    tokio::spawn(async move {
        let _lease = lease;
        let file = file.into_std().await;
        let _ = tokio::task::spawn_blocking(move || {
            let writer = ChunkWriter::new(sender.clone(), cancellation.clone());
            let source = std::io::BufReader::new(file);
            let written = match format {
                ArtifactFormat::TarGz => downloads::to_tar_gz(source, writer).map(drop),
                ArtifactFormat::Zip => downloads::to_zip(source, writer).map(drop),
            };
            if let Err(error) = written {
                tokio::runtime::Handle::current().block_on(async {
                    tokio::select! {
                        () = cancellation.cancelled() => {},
                        _ = sender.send(Err(error)) => {},
                    }
                });
            }
        })
        .await;
    });
    let stream = futures_util::stream::unfold(receiver, |mut receiver| async move { receiver.recv().await.map(|item| (item, receiver)) });
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    // Names are [A-Za-z0-9._-], so they need no quoting beyond the quotes.
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{}.{extension}\"", artifact.name)) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

/// Sends what the zip writer writes as chunks of the response body; a closed response
/// (the client went away) stops the conversion.
pub(crate) struct ChunkWriter {
    sender: tokio::sync::mpsc::Sender<std::io::Result<axum::body::Bytes>>,
    chunk: Vec<u8>,
    cancellation: tokio_util::sync::CancellationToken,
    runtime: tokio::runtime::Handle,
}

impl ChunkWriter {
    const SIZE: usize = 64 * 1024;

    // Actions: run logs.
    pub(crate) fn new(
        sender: tokio::sync::mpsc::Sender<std::io::Result<axum::body::Bytes>>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self { sender, chunk: Vec::with_capacity(Self::SIZE), cancellation, runtime: tokio::runtime::Handle::current() }
    }

    fn send(&mut self) -> std::io::Result<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::replace(&mut self.chunk, Vec::with_capacity(Self::SIZE));
        self.runtime.block_on(async {
            tokio::select! {
                () = self.cancellation.cancelled() => Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "plugin disabled")),
                result = self.sender.send(Ok(chunk.into())) => result.map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the download was closed")),
            }
        })
    }
}

impl std::io::Write for ChunkWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancellation.is_cancelled() {
            // `write_all` retries Interrupted indefinitely; cancellation must be terminal.
            return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "plugin disabled"));
        }
        self.chunk.extend_from_slice(buf);
        if self.chunk.len() >= Self::SIZE {
            self.send()?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

/// Every page of Actions, with the methods it answers.
pub const PATHS: &[(&str, &[Method])] = &[
    ("/{owner}/{repo}/actions", &[Method::GET]),
    ("/{owner}/{repo}/actions/workflows/{workflow}", &[Method::GET]),
    ("/{owner}/{repo}/actions/workflows/{workflow}/badge.svg", &[Method::GET]),
    ("/{owner}/{repo}/actions/workflows/{workflow}/dispatch", &[Method::POST]),
    // Actions: disabled workflows.
    ("/{owner}/{repo}/actions/workflows/{workflow}/switch", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/cancel", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/rerun", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/rerun-failed", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/approve", &[Method::POST]),
    // Actions: environments.
    ("/{owner}/{repo}/actions/runs/{run}/review", &[Method::POST]),
    ("/{owner}/{repo}/settings/actions/environments", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/environments/{environment}", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/environments/{environment}/secrets", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/environments/{environment}/variables", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/environments/{environment}/delete", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/rerun", &[Method::POST]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw", &[Method::GET]),
    // Actions: artifacts (A).
    ("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}", &[Method::GET]),
    ("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}/{format}", &[Method::GET]),
    ("/{owner}/{repo}/settings/actions", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/runners/{runner}", &[Method::POST]),
    ("/{owner}/{repo}/settings/actions/secrets", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/variables", &[Method::GET, Method::POST]),
    ("/{owner}/{repo}/settings/actions/trusted", &[Method::POST]),
    ("/{owner}/settings/actions", &[Method::GET, Method::POST]),
    ("/{owner}/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/{owner}/settings/actions/runners/{runner}", &[Method::POST]),
    ("/settings/actions", &[Method::GET, Method::POST]),
    ("/settings/actions/runners", &[Method::GET, Method::POST]),
    ("/settings/actions/runners/{runner}", &[Method::POST]),
    ("/admin/runners", &[Method::GET, Method::POST]),
    ("/admin/runners/{runner}", &[Method::POST]),
];

// Actions: artifacts (A).
/// Artifact downloads. Merged outside the page router (no 30-second timeout, no
/// compression: archives are already gzipped), only when Actions is enabled, so this
/// router applies the session, rate-limit, CSRF and security-header middleware itself.
pub fn file_routes(state: &AppState) -> Router<AppState> {
    use axum::middleware::{from_fn, from_fn_with_state};
    Router::new()
        .route("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}", get(runs::artifact))
        .route("/{owner}/{repo}/actions/runs/{run}/artifacts/{artifact}/{format}", get(runs::artifact_as))
        .layer(from_fn_with_state(state.clone(), crate::middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), crate::middleware::session))
        .layer(from_fn_with_state(state.clone(), crate::middleware::rate_limit))
        .layer(from_fn(crate::middleware::csrf_origin_check))
        .layer(from_fn_with_state(crate::middleware::SecurityHeaders::new(&state.config), crate::middleware::security_headers))
}

/// The Actions pages. Merged only when Actions is enabled (see `routes::page_routes`).
pub fn routes() -> Router<AppState> {
    // Compatibility GETs never execute mutations or render native documents.
    let mut router = Router::new()
        .route("/{owner}/{repo}/actions/workflows/{workflow}/badge.svg", get(runs::badge))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw", get(jobs::raw));
    for (path, methods) in PATHS {
        if methods.contains(&Method::GET) && !path.contains("artifacts/") && !path.ends_with("badge.svg") && !path.ends_with("logs/raw") {
            router = router.route(path, get(compatibility));
        }
    }
    router
}

async fn compatibility(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(values): axum::extract::Path<std::collections::HashMap<String, String>>,
    matched: axum::extract::MatchedPath,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> crate::error::WebResult<axum::response::Redirect> {
    use crate::error::WebError;
    use rbit_services::plugin_pages::{PageQuery, PageScope};
    if uri.query().is_some_and(|query| query.len() > 8192) {
        return Err(WebError::BadRequest("query is too large".into()));
    }
    let axum::extract::Query(query) =
        axum::extract::Query::<Vec<(String, String)>>::try_from_uri(&uri).map_err(|_| WebError::BadRequest("invalid query".into()))?;
    let pattern = matched.as_str();
    let repository = values.get("repo");
    let owner = values.get("owner");
    let mut pairs = Vec::new();
    let path = if let Some(repo) = repository {
        let _ = repo;
        if pattern.ends_with("/environments/{environment}")
            || pattern.ends_with("/environments/{environment}/secrets")
            || pattern.ends_with("/environments/{environment}/variables")
        {
            pairs.push(("environment".into(), values.get("environment").ok_or(WebError::NotFound)?.clone()));
            "settings/environment"
        } else if pattern.ends_with("/environments") {
            "settings/environments"
        } else if pattern.ends_with("/settings/actions/runners") {
            "settings/runners"
        } else if pattern.contains("/settings/actions") {
            "settings"
        } else if pattern.ends_with("/jobs/{job}/logs") {
            pairs.extend([("run".into(), values["run"].clone()), ("job".into(), values["job"].clone())]);
            if query.iter().any(|(key, _)| key == "step") { "logs" } else { "job" }
        } else if pattern.ends_with("/jobs/{job}") {
            pairs.extend([("run".into(), values["run"].clone()), ("job".into(), values["job"].clone())]);
            "job"
        } else if pattern.contains("/runs/{run}") {
            pairs.push(("run".into(), values["run"].clone()));
            "run"
        } else if pattern.contains("/workflows/{workflow}") {
            pairs.push(("workflow".into(), values["workflow"].clone()));
            "workflow"
        } else {
            "runs"
        }
    } else if let Some(owner) = owner {
        pairs.push(("owner".into(), owner.clone()));
        if pattern.ends_with("/runners") { "org/runners" } else { "org/settings" }
    } else if pattern == "/admin/runners" {
        "admin/runners"
    } else if pattern.ends_with("/runners") {
        "settings/runners"
    } else {
        "settings"
    };
    let scope = if repository.is_some() { PageScope::Repository } else { PageScope::Global };
    let id = "rbit.actions".parse().map_err(WebError::internal)?;
    let route = state.services.page_registry().snapshot().find(&id, scope, path).ok_or(WebError::NotFound)?;
    for (key, value) in query {
        if route.descriptor().query().allows(&key) {
            pairs.push((key, value));
        }
    }
    let query = PageQuery::new(pairs).map_err(|_| WebError::BadRequest("invalid query".into()))?.encoded();
    let target =
        route.descriptor().target(&id).render(owner.map(String::as_str), repository.map(String::as_str)).map_err(WebError::internal)?;
    let target = if query.is_empty() { target } else { format!("{target}?{query}") };
    Ok(axum::response::Redirect::to(&target))
}

#[cfg(test)]
mod tests {
    use super::ChunkWriter;
    use std::{io::Write as _, time::Duration};

    #[test]
    fn disabling_interrupts_an_archive_writer_even_when_the_client_does_not_read() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(async {
                let (sender, _receiver) = tokio::sync::mpsc::channel(1);
                sender.try_send(Ok(axum::body::Bytes::from_static(b"full"))).unwrap();
                let cancellation = tokio_util::sync::CancellationToken::new();
                let mut writer = ChunkWriter::new(sender, cancellation.clone());
                let (started, ready) = tokio::sync::oneshot::channel();
                let mut producer = tokio::task::spawn_blocking(move || {
                    started.send(()).unwrap();
                    writer.write_all(&vec![0; ChunkWriter::SIZE])
                });
                ready.await.unwrap();
                assert!(tokio::time::timeout(Duration::from_millis(20), &mut producer).await.is_err());
                cancellation.cancel();
                let result = tokio::time::timeout(Duration::from_secs(5), producer).await.unwrap().unwrap();
                assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
            });
        }));
        // A regression must fail within a deadline, rather than hang runtime teardown
        // forever waiting for an uninterruptible blocking writer.
        runtime.shutdown_timeout(Duration::from_secs(1));
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}
