//! Advertisement owns admission through cancellation cleanup, independently of its waiter.
use super::{Denied, TOKEN_REQUIRED, admit, authorize, busy, git_protocol, remote_error};
use crate::{AppState, client_ip::Client};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_transport::{ProcessControl, Service, TransportError};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
pub struct InfoRefsQuery {
    service: Option<String>,
}

/// `GET /{owner}/{repo}.git/info/refs?service=git-upload-pack|git-receive-pack`
pub async fn info_refs(
    State(state): State<AppState>,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<InfoRefsQuery>,
    client: Client,
    headers: HeaderMap,
) -> Response {
    let Some(service) = query.service.as_deref().and_then(Service::from_wire_name) else {
        return (StatusCode::FORBIDDEN, "the dumb HTTP protocol is not supported; please upgrade git").into_response();
    };
    let ctx = match authorize(&state, &client, &headers, &owner, &repo, service).await {
        Ok((ctx, _)) => ctx,
        // The advertisement is the first request of every operation, so the message reaches the user.
        Err(Denied::TokenRequired) => return remote_error(service, TOKEN_REQUIRED),
        Err(denied) => return denied.into_response(),
    };
    let slot = match admit(&state, &client) {
        Ok(slot) => slot,
        Err(refused) => return busy(refused),
    };

    let control = ProcessControl::new(CancellationToken::new(), tokio::time::Instant::now() + state.config.repository.git_idle_timeout());
    let result = state.transport.advertise_refs_owned(service, &ctx.disk_path, git_protocol(&headers), control, slot).await;
    advertisement_response(result, service, &owner, &repo)
}

fn advertisement_response(result: Result<Vec<u8>, TransportError>, service: Service, owner: &str, repo: &str) -> Response {
    match result {
        Ok(body) => ([(header::CONTENT_TYPE, service.advertisement_content_type()), (header::CACHE_CONTROL, "no-cache".to_owned())], body)
            .into_response(),
        Err(TransportError::TimedOut) => {
            tracing::warn!(%owner, %repo, %service, "ref advertisement exceeded the time limit");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        Err(error) => {
            tracing::error!(%error, %owner, %repo, "ref advertisement failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deadline_keeps_the_existing_503_without_authentication_challenge_or_private_details() {
        let response = advertisement_response(Err(TransportError::TimedOut), Service::UploadPack, "private-owner", "private-project");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
        assert!(axum::body::to_bytes(response.into_body(), 1024).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn successful_advertisement_preserves_wire_content_type_and_cache_header() {
        let response = advertisement_response(Ok(b"0000".to_vec()), Service::ReceivePack, "alice", "project");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], Service::ReceivePack.advertisement_content_type().as_str());
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(axum::body::to_bytes(response.into_body(), 1024).await.unwrap().as_ref(), b"0000");
    }
}
