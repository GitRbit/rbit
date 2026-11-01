//! Bounded package upload and journal recovery; never local/offline installation.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context as _, bail};
use clap::Subcommand;
use rbit_plugin_packages::MAX_ARCHIVE_WIRE_BYTES;
use rbit_services::PackageOperationId as OperationId;
use reqwest::{Method, StatusCode};
use serde_json::Value;
use tokio::io::AsyncReadExt as _;
use tokio_util::io::ReaderStream;

use super::{Api, failure};

const RESPONSE_LIMIT: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(70);
const FILE_LIMIT: u64 = MAX_ARCHIVE_WIRE_BYTES - 1;

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Review passive package policy; never start or enable a plugin.
    Policy {
        #[command(subcommand)]
        command: super::policy::PolicyCommand,
    },
    /// Stream an archive to an administrator endpoint; neither enable nor trust it.
    Install {
        file: PathBuf,
        /// Opaque retry/recovery key; generated and printed before upload by default.
        #[arg(long)]
        operation_id: Option<String>,
    },
    /// Inspect the durable journal using the key retained before upload.
    Operation { id: String },
}

pub(super) async fn run(api: &Api, command: PluginCommand) -> anyhow::Result<i32> {
    let (status, value) = match command {
        PluginCommand::Policy { command } => return super::policy::run(api, command).await,
        PluginCommand::Install { file, operation_id } => {
            let id = match operation_id {
                Some(id) => parse_id(&id)?,
                None => fresh_id()?,
            };
            let source = tokio::fs::File::open(&file).await.with_context(|| format!("cannot open {}", file.display()))?;
            let metadata = source.metadata().await.context("cannot inspect package source")?;
            if !metadata.is_file() || metadata.len() > FILE_LIMIT {
                bail!("package source must be a regular file within the archive wire limit");
            }
            eprintln!("Package operation: {}", id.as_str());
            eprintln!("Recover with: rbit plugin operation -- {}", id.as_str());
            eprintln!("Use the same instance (--hostname) as this upload.");
            upload(api, &id, source)
                .await
                .with_context(|| format!("package operation {} may require status inspection; no automatic retry was made", id.as_str()))?
        }
        PluginCommand::Operation { id } => {
            let id = parse_id(&id)?;
            let response =
                api.request(Method::GET, &path(&id))?.timeout(REQUEST_TIMEOUT).send().await.context("cannot inspect package operation")?;
            answer(response, &id).await?
        }
    };
    if !status.is_success() {
        bail!("{}", failure(status, &value.to_string()));
    }
    let code = outcome(status, &value)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(code)
}

pub(super) fn parse_id(value: &str) -> anyhow::Result<OperationId> {
    OperationId::new(value).map_err(|_| anyhow::anyhow!("operation ID must contain 1-64 lowercase ASCII letters, digits or hyphens"))
}

pub(super) fn fresh_id() -> anyhow::Result<OperationId> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).context("cannot generate package operation ID")?;
    let hex = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(hex[usize::from(byte >> 4)]));
        value.push(char::from(hex[usize::from(byte & 15)]));
    }
    parse_id(&value)
}

fn path(id: &OperationId) -> String {
    format!("admin/plugin-package-operations/{}", id.as_str())
}

async fn upload(api: &Api, id: &OperationId, source: tokio::fs::File) -> anyhow::Result<(StatusCode, Value)> {
    // Never collect the archive, follow a server-provided destination, or retry a
    // possibly committed request. The extra byte permits the host to reject growth
    // past the maximum rather than silently truncating an overlarge source.
    let stream = ReaderStream::with_capacity(source.take(FILE_LIMIT + 1), 64 * 1024);
    let response = api
        .request(Method::PUT, &path(id))?
        .timeout(REQUEST_TIMEOUT)
        .header("Content-Type", "application/octet-stream")
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .context("package upload response unavailable")?;
    answer(response, id).await
}

async fn answer(mut response: reqwest::Response, id: &OperationId) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    if response.content_length().is_some_and(|size| size > RESPONSE_LIMIT as u64) {
        bail!("package operation response exceeds 64 KiB");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.context("cannot read package operation response")? {
        if chunk.len() > RESPONSE_LIMIT - bytes.len() {
            bail!("package operation response exceeds 64 KiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).context("package operation response is not JSON")?;
    if status.is_success() && value["operation_id"].as_str() != Some(id.as_str()) {
        bail!("package operation response identity does not match the requested key");
    }
    Ok((status, value))
}

fn outcome(status: StatusCode, value: &Value) -> anyhow::Result<i32> {
    match (status, value["stage"].as_str()) {
        (StatusCode::OK, Some("complete")) => Ok(0),
        (StatusCode::OK | StatusCode::ACCEPTED, Some("pending" | "awaiting_durability")) => Ok(8),
        (StatusCode::OK, Some("error")) => Ok(1),
        _ => bail!("unexpected package operation status; inspect the retained operation ID"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use serde_json::json;

    #[test]
    fn package_commands_parse_and_keys_cannot_change_the_route() {
        let cli = crate::cli::Cli::try_parse_from([
            "rbit",
            "plugin",
            "install",
            "bundle.rbit-plugin",
            "--operation-id",
            "retry-12",
            "--hostname",
            "https://example.test/deploy",
        ])
        .unwrap();
        assert!(
            matches!(cli.command, Some(crate::cli::Command::Client(super::super::ClientCommand::Plugin { command: PluginCommand::Install { operation_id: Some(ref id), .. }, .. })) if id == "retry-12")
        );
        assert!(crate::cli::Cli::try_parse_from(["rbit", "plugin", "operation", "retry-12"]).is_ok());
        assert!(crate::cli::Cli::try_parse_from(["rbit", "plugin", "operation", "--", "-retry"]).is_ok());
        for invalid in ["", "../escape", "a/b", "a?token", "UPPER", &"a".repeat(65)] {
            assert!(parse_id(invalid).is_err());
        }
        let id = fresh_id().unwrap();
        assert_eq!(id.as_str().len(), 32);
        assert_eq!(path(&parse_id("retry-12").unwrap()), "admin/plugin-package-operations/retry-12");
    }

    #[test]
    fn awaiting_durability_is_pending_and_never_install_success() {
        assert_eq!(outcome(StatusCode::ACCEPTED, &json!({"stage":"awaiting_durability"})).unwrap(), 8);
        assert_eq!(outcome(StatusCode::OK, &json!({"stage":"complete"})).unwrap(), 0);
        assert_eq!(outcome(StatusCode::OK, &json!({"stage":"error"})).unwrap(), 1);
        assert!(outcome(StatusCode::ACCEPTED, &json!({"stage":"complete"})).is_err());
    }

    struct LocalServer(tokio::task::JoinHandle<()>);
    impl Drop for LocalServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    #[tokio::test]
    async fn file_is_streamed_to_fixed_base_path_and_known_id_recovers_pending_response() {
        use axum::{
            body::{Body, to_bytes},
            http::{HeaderMap, Uri},
            response::IntoResponse,
            routing::any,
        };
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let base = format!("http://{}/deploy", listener.local_addr().unwrap());
        let (sender, mut captured) = tokio::sync::mpsc::channel(2);
        let app = axum::Router::new().fallback(any(move |method: Method, uri: Uri, headers: HeaderMap, body: Body| {
            let sender = sender.clone();
            async move {
                let bytes = to_bytes(body, 256 * 1024).await.unwrap();
                sender.send((method.clone(), uri.path().to_owned(), headers, bytes)).await.unwrap();
                let (status, stage) =
                    if method == Method::PUT { (StatusCode::ACCEPTED, "awaiting_durability") } else { (StatusCode::OK, "complete") };
                (status, axum::Json(json!({"operation_id":"known-before-upload", "stage":stage}))).into_response()
            }
        }));
        let _server = LocalServer(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
        rbit_services::webhooks::delivery::install_crypto_provider();
        let api = Api::new(&base, Some("private-admin-token".to_owned())).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("source.rbit-plugin");
        let bytes: Vec<u8> = (0..150_000).map(|index| u8::try_from(index % 251).unwrap()).collect();
        tokio::fs::write(&file, &bytes).await.unwrap();
        let id = parse_id("known-before-upload").unwrap();
        let (status, value) = upload(&api, &id, tokio::fs::File::open(&file).await.unwrap()).await.unwrap();
        assert_eq!(outcome(status, &value).unwrap(), 8);
        let (method, route, headers, received) = captured.recv().await.unwrap();
        assert_eq!(method, Method::PUT);
        assert_eq!(route, "/deploy/api/v1/admin/plugin-package-operations/known-before-upload");
        assert_eq!(headers["authorization"], "Bearer private-admin-token");
        assert_eq!(headers["content-type"], "application/octet-stream");
        assert_eq!(received.as_ref(), bytes.as_slice());
        let response = api.request(Method::GET, &path(&id)).unwrap().send().await.unwrap();
        let (status, recovered) = answer(response, &id).await.unwrap();
        assert_eq!(outcome(status, &recovered).unwrap(), 0);
        assert_eq!(captured.recv().await.unwrap().0, Method::GET);
        assert!(captured.try_recv().is_err(), "no automatic replay");
        let wrong = parse_id("wrong-key").unwrap();
        let error = upload(&api, &wrong, tokio::fs::File::open(&file).await.unwrap()).await.unwrap_err();
        assert!(error.to_string().contains("identity does not match"));
        assert!(captured.recv().await.unwrap().1.ends_with("/wrong-key"));
    }

    #[tokio::test]
    async fn operation_response_is_bounded_even_without_content_length() {
        use axum::{body::Body, routing::get};
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/api/v1/admin/plugin-package-operations/response-limit",
            get(|| async { Body::from_stream(ReaderStream::new(std::io::Cursor::new(vec![b'x'; RESPONSE_LIMIT + 1]))) }),
        );
        let _server = LocalServer(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
        rbit_services::webhooks::delivery::install_crypto_provider();
        let api = Api::new(&base, None).unwrap();
        let response = api.request(Method::GET, &path(&parse_id("response-limit").unwrap())).unwrap().send().await.unwrap();
        assert_eq!(response.content_length(), None);
        assert!(answer(response, &parse_id("response-limit").unwrap()).await.unwrap_err().to_string().contains("exceeds 64 KiB"));
    }
}
