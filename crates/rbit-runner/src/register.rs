//! `rbit-runner register`: exchanges a registration token for a runner credential.
//!
//! The configuration is checked in full before the token is spent, since a registration
//! token works once. On success `runner.toml` and `.credential` are written; the
//! credential is shown nowhere else and the server keeps only its digest.

use std::path::Path;

use anyhow::{Context as _, bail};
use rbit_workflow::{protocol::RegisterRequest, secret::Secret};

use crate::{
    cli::RegisterArgs,
    client::Client,
    config::{self, ContainerConfig, RunnerConfig},
};

/// The configuration `args` describe, validated.
pub fn configuration(args: &RegisterArgs) -> anyhow::Result<RunnerConfig> {
    let server = config::server_url(&args.url, args.insecure_http)?;
    let labels = if args.labels.is_empty() { vec![config::DEFAULT_LABEL.to_owned()] } else { args.labels.clone() };
    let config = RunnerConfig {
        url: server.as_str().trim_end_matches('/').to_owned(),
        insecure_http: args.insecure_http,
        name: args.name.clone().unwrap_or_else(config::host_name),
        capacity: args.capacity,
        work_dir: args.work_dir.clone(),
        shutdown_grace_seconds: 600,
        labels: labels.iter().map(|label| config::parse_label(label)).collect::<anyhow::Result<_>>()?,
        container: ContainerConfig { program: args.container_program.clone(), ..ContainerConfig::default() },
        cache: config::CacheConfig::default(),
    };
    config.validate()?;
    Ok(config)
}

pub async fn run(config_path: &Path, args: RegisterArgs) -> anyhow::Result<()> {
    let credential_path = config::credential_path(config_path);
    if credential_path.exists() && !args.replace {
        bail!(
            "{} already exists, so this directory holds a registered runner; pass --replace to register a new one \
             (remove the old runner on the runners page)",
            credential_path.display()
        );
    }
    let config = configuration(&args)?;
    if let Some(warning) = config::host_warning(&config) {
        eprintln!("{warning}");
    }
    let client = Client::new(&config::server_url(&config.url, config.insecure_http)?, None)?;
    let request = RegisterRequest {
        token: Secret::new(args.token),
        name: config.name.clone(),
        labels: config.declared_labels(),
        capabilities: config.capabilities(false),
    };
    request.validate().map_err(|error| anyhow::anyhow!("{error}"))?;
    let response = client.register(&request).await.context("registration failed")?;
    drop(request);

    config::save(config_path, &config)?;
    config::write_credential(&credential_path, &response.credential)?;
    println!(
        "Registered runner `{}` (id {}) with {}.\nConfiguration: {}\nCredential:    {}\nStart it with `rbit-runner run`.",
        config.name,
        response.runner_id.0,
        config.url,
        config_path.display(),
        credential_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{Json, Router, http::HeaderMap, routing::post};
    use rbit_workflow::protocol::{Backend, ErrorBody, ErrorCode, Intervals, RegisterResponse, RunnerId};

    use super::*;

    const CREDENTIAL: &str = "rbrn_CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";

    fn args(url: &str) -> RegisterArgs {
        RegisterArgs {
            url: url.to_owned(),
            token: "registration-token".into(),
            name: Some("build-1".into()),
            labels: vec!["ubuntu-latest:container:docker.io/library/ubuntu:24.04".into(), "self-hosted:host".into()],
            capacity: 2,
            work_dir: "work".into(),
            container_program: "podman".into(),
            insecure_http: true,
            replace: false,
        }
    }

    /// A server that answers one registration and records the request and its headers.
    async fn server(seen: Arc<Mutex<Vec<(HeaderMap, RegisterRequest)>>>) -> String {
        let app = Router::new().route(
            "/api/actions/runner/v1/register",
            post(move |headers: HeaderMap, Json(request): Json<RegisterRequest>| async move {
                let token = request.token.expose().to_owned();
                seen.lock().unwrap().push((headers, request));
                if token == "registration-token" {
                    Ok(Json(RegisterResponse {
                        runner_id: RunnerId(7),
                        credential: Secret::new(CREDENTIAL),
                        intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
                    }))
                } else {
                    Err((axum::http::StatusCode::UNAUTHORIZED, Json(ErrorBody::new(ErrorCode::Unauthenticated, "unknown token"))))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn registration_writes_the_configuration_and_credential() {
        let seen = Arc::default();
        let url = server(Arc::clone(&seen)).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runner.toml");
        run(&path, args(&url)).await.unwrap();

        let loaded = config::load(&path).unwrap();
        assert_eq!(loaded.config.url, url);
        assert!(loaded.config.insecure_http);
        assert_eq!((loaded.config.capacity, loaded.config.container.program.as_str()), (2, "podman"));
        assert_eq!(config::read_credential(&loaded.credential_path).unwrap().expose(), CREDENTIAL);

        {
            let seen = seen.lock().unwrap();
            let (headers, request) = &seen[0];
            assert_eq!(headers["rbit-runner-protocol"], "1");
            assert_eq!(headers["user-agent"], crate::client::user_agent());
            assert!(!headers.contains_key("authorization"));
            assert_eq!(request.name, "build-1");
            assert_eq!(request.capabilities.backends, [Backend::Container, Backend::Host]);
            assert_eq!(request.capabilities.os, std::env::consts::OS);
        }

        // A second registration in the same directory needs --replace.
        let error = run(&path, args(&url)).await.unwrap_err().to_string();
        assert!(error.contains("--replace"), "{error}");
        let mut replace = args(&url);
        replace.replace = true;
        replace.token = "spent".into();
        let error = format!("{:#}", run(&path, replace).await.unwrap_err());
        assert!(error.contains("401") && error.contains("unknown token"), "{error}");
        // The refused attempt left the first registration in place.
        assert_eq!(config::read_credential(&loaded.credential_path).unwrap().expose(), CREDENTIAL);
    }

    #[tokio::test]
    async fn bad_configurations_never_spend_the_token() {
        let seen = Arc::default();
        let url = server(Arc::clone(&seen)).await;
        let dir = tempfile::tempdir().unwrap();
        let mut bad = args(&url);
        bad.labels = vec!["ubuntu-latest:container".into()];
        assert!(run(&dir.path().join("runner.toml"), bad).await.is_err());
        let mut plain = args(&url.replace("127.0.0.1", "example.com"));
        plain.insecure_http = false;
        assert!(run(&dir.path().join("runner.toml"), plain).await.unwrap_err().to_string().contains("https"));
        assert!(seen.lock().unwrap().is_empty());
        assert!(!dir.path().join("runner.toml").exists());
    }

    #[test]
    fn defaults_never_pick_the_host_backend() {
        let mut defaults = args("https://git.example.com");
        defaults.labels.clear();
        let config = configuration(&defaults).unwrap();
        assert_eq!(config.backends(), [Backend::Container]);
        assert!(config::host_warning(&config).is_none());
    }
}
