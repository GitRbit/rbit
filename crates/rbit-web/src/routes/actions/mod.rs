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

mod environments;
pub(crate) mod jobs;
mod runners;
mod runs;
mod settings;

use axum::{
    Router,
    http::Method,
    routing::{get, post},
};

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
    let (kind, extension) = match format {
        ArtifactFormat::TarGz => ("application/gzip", "tar.gz"),
        ArtifactFormat::Zip => ("application/zip", "zip"),
    };
    let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(8);
    tokio::spawn(async move {
        let file = file.into_std().await;
        let _ = tokio::task::spawn_blocking(move || {
            let writer = ChunkWriter { sender: sender.clone(), chunk: Vec::with_capacity(ChunkWriter::SIZE) };
            let source = std::io::BufReader::new(file);
            let written = match format {
                ArtifactFormat::TarGz => downloads::to_tar_gz(source, writer).map(drop),
                ArtifactFormat::Zip => downloads::to_zip(source, writer).map(drop),
            };
            if let Err(error) = written {
                let _ = sender.blocking_send(Err(error));
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
struct ChunkWriter {
    sender: tokio::sync::mpsc::Sender<std::io::Result<axum::body::Bytes>>,
    chunk: Vec<u8>,
}

impl ChunkWriter {
    const SIZE: usize = 64 * 1024;

    fn send(&mut self) -> std::io::Result<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        let chunk = std::mem::replace(&mut self.chunk, Vec::with_capacity(Self::SIZE));
        self.sender
            .blocking_send(Ok(chunk.into()))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the download was closed"))
    }
}

impl std::io::Write for ChunkWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
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
    // Actions: interface and secrets (U).
    Router::new()
        .route("/{owner}/{repo}/actions", get(runs::list))
        .route("/{owner}/{repo}/actions/workflows/{workflow}", get(runs::workflow))
        .route("/{owner}/{repo}/actions/workflows/{workflow}/badge.svg", get(runs::badge))
        .route("/{owner}/{repo}/actions/workflows/{workflow}/dispatch", post(runs::dispatch))
        // Actions: disabled workflows.
        .route("/{owner}/{repo}/actions/workflows/{workflow}/switch", post(runs::switch))
        .route("/{owner}/{repo}/actions/runs/{run}", get(runs::show))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs", get(runs::jobs))
        .route("/{owner}/{repo}/actions/runs/{run}/cancel", post(runs::cancel))
        .route("/{owner}/{repo}/actions/runs/{run}/rerun", post(runs::rerun_all))
        .route("/{owner}/{repo}/actions/runs/{run}/rerun-failed", post(runs::rerun_failed))
        .route("/{owner}/{repo}/actions/runs/{run}/approve", post(runs::approve))
        // Actions: environments.
        .route("/{owner}/{repo}/actions/runs/{run}/review", post(runs::review))
        .route("/{owner}/{repo}/settings/actions/environments", get(environments::list).post(environments::create))
        .route("/{owner}/{repo}/settings/actions/environments/{environment}", get(environments::show).post(environments::save))
        .route(
            "/{owner}/{repo}/settings/actions/environments/{environment}/secrets",
            get(environments::to_environment).post(environments::secret),
        )
        .route(
            "/{owner}/{repo}/settings/actions/environments/{environment}/variables",
            get(environments::to_environment).post(environments::variable),
        )
        .route("/{owner}/{repo}/settings/actions/environments/{environment}/delete", post(environments::delete))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/rerun", post(runs::rerun_job))
        .route("/{owner}/{repo}/settings/actions/runners", get(runners::repository).post(runners::repository_tokens))
        .route("/{owner}/{repo}/settings/actions/runners/{runner}", post(runners::repository_runner))
        .route("/{owner}/settings/actions/runners", get(runners::organization).post(runners::organization_tokens))
        .route("/{owner}/settings/actions/runners/{runner}", post(runners::organization_runner))
        .route("/settings/actions/runners", get(runners::personal).post(runners::personal_tokens))
        .route("/settings/actions/runners/{runner}", post(runners::personal_runner))
        .route("/admin/runners", get(runners::instance).post(runners::instance_tokens))
        .route("/admin/runners/{runner}", post(runners::instance_runner))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}", get(jobs::show))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs", get(jobs::logs))
        .route("/{owner}/{repo}/actions/runs/{run}/jobs/{job}/logs/raw", get(jobs::raw))
        .route("/{owner}/{repo}/settings/actions", get(settings::repository).post(settings::save_repository))
        .route("/{owner}/{repo}/settings/actions/secrets", get(settings::to_repository).post(settings::repository_secret))
        .route("/{owner}/{repo}/settings/actions/variables", get(settings::to_repository).post(settings::repository_variable))
        .route("/{owner}/{repo}/settings/actions/trusted", post(settings::untrust))
        .route("/{owner}/settings/actions", get(settings::organization).post(settings::save_organization))
        .route("/settings/actions", get(settings::personal).post(settings::save_personal))
}
