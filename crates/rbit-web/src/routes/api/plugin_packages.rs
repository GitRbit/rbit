//! Administrator-only archive ingress. This records immutable packages; it does
//! not select a runtime version, activate code, approve trust or change grants.
//! An administrator/CLI chooses the opaque idempotency key before uploading.
//! A missing journal row does not prove there is no active staging worker.

use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::{Stream, StreamExt, TryStreamExt};
use rbit_core::{TokenScope, User};
use rbit_plugin_packages::{ArchiveError, CleanupStatus, MAX_ARCHIVE_WIRE_BYTES, PublicationVisibility, StoreError, StoreFailure};
use rbit_services::{PackageInstallOutcome, ServiceError, webhooks::payloads};
use rbit_services::{
    PackageInstallationDescriptor as InstallationDescriptor, PackageInstallationOperation as InstallationOperation,
    PackageOperationError as OperationError, PackageOperationId as OperationId, PackageOperationStage as OperationStage,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, ReadBuf},
    time::Sleep,
};
use tokio_util::{
    io::{StreamReader, SyncIoBridge},
    sync::CancellationToken,
};

use super::{
    ApiMethod::{Get, Put},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    route,
};
use crate::AppState;

const TIMEOUT: Duration = Duration::from_mins(1);
// The validator's quota reserves one byte of capacity for its actual-EOF probe.
const MAX_BODY_BYTES: u64 = MAX_ARCHIVE_WIRE_BYTES - 1;
const PATH: &str = "/admin/plugin-package-operations/{operation_id}";

pub(super) fn routes() -> Vec<ApiRoute> {
    vec![route(Put, PATH, upload).streaming(), route(Get, PATH, show)]
}

fn administrator(caller: &Caller) -> ApiResult<&User> {
    let user = caller.require(TokenScope::Admin)?;
    if !user.is_admin {
        return Err(ApiError::forbidden("Active site administrator required"));
    }
    Ok(user)
}

fn operation_id(value: &str) -> ApiResult<OperationId> {
    OperationId::new(value).map_err(|_| ApiError::invalid("operation_id", "Expected 1-64 lowercase ASCII letters, digits or hyphens"))
}

fn location(state: &AppState, id: &OperationId) -> String {
    format!("{}/api/v1/admin/plugin-package-operations/{}", state.config.server.base_url().trim_end_matches('/'), id.as_str())
}

fn located(mut error: ApiError, location: &str) -> ApiError {
    error.location = Some(location.to_owned());
    error
}

async fn show(State(state): State<AppState>, caller: Caller, Path(raw): Path<String>) -> ApiResult<Json<Value>> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let url = location(&state, &id);
    // The service reloads the active administrator before querying the ledger.
    let operation = state
        .services
        .packages
        .operation(actor, &id)
        .await
        .map_err(|error| located(fault(&error, Fault::None), &url))?
        .ok_or_else(|| located(ApiError::not_found(), &url))?;
    Ok(Json(operation_value(&operation, &url)))
}

async fn upload(
    State(state): State<AppState>,
    caller: Caller,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let url = location(&state, &id);
    // Store/Service contracts assume host-proven private ACLs on Windows. The
    // HTTP composition has no supported verifier yet, so it cannot assert that
    // prerequisite merely because an administrator authenticated.
    if cfg!(windows) {
        return Err(located(
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Windows package upload requires supported host root verification"),
            &url,
        ));
    }
    check_headers(&headers).map_err(|error| located(error, &url))?;
    let deadline = Instant::now() + TIMEOUT;
    let cancel = CancellationToken::new();
    let _waiter = CancelOnDrop(cancel.clone());
    let source_fault = Arc::new(AtomicU8::new(Fault::None as u8));
    let stream = cooperative_frames(body.into_data_stream().map_err(|_| io::Error::other("package request body failed")));
    let reader = BoundedSource::new(StreamReader::new(stream), cancel, deadline, MAX_BODY_BYTES, Arc::clone(&source_fault));
    // Constructed inside the runtime, read only by the service's owned blocking
    // worker. No upload pump or eager read can bypass its two-operation admission.
    // install rechecks live administrator status BEFORE reading any source byte.
    let bridge = SyncIoBridge::new(reader);
    let install = state.services.packages.install(actor, id, bridge, deadline, MAX_ARCHIVE_WIRE_BYTES);
    let outcome = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), install)
        .await
        .map_err(|_| located(ApiError::new(StatusCode::REQUEST_TIMEOUT, "Package operation deadline expired; inspect its status"), &url))?
        .map_err(|error| located(fault(&error, Fault::from_code(source_fault.load(Ordering::Relaxed))), &url))?;
    let (status, value) = match outcome {
        PackageInstallOutcome::Installed(installation) => (
            StatusCode::OK,
            json!({
                "operation_id": raw, "url": url, "stage": "complete", "package": descriptor_value(&installation.descriptor),
                "installed_at": payloads::timestamp(installation.installed_at),
            }),
        ),
        PackageInstallOutcome::AwaitingDurability { operation, .. } => (StatusCode::ACCEPTED, operation_value(&operation, &url)),
    };
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&url).map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Invalid configured public URL"))?,
    );
    Ok(response)
}

fn check_headers(headers: &HeaderMap) -> ApiResult<()> {
    // No decompression: quotas cover exactly the bytes validated and stored.
    let encodings: Vec<_> = headers.get_all(header::CONTENT_ENCODING).iter().collect();
    if encodings.len() > 1 || encodings.first().is_some_and(|value| value.as_bytes() != b"identity") {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Only identity content encoding is supported"));
    }
    let types: Vec<_> = headers.get_all(header::CONTENT_TYPE).iter().collect();
    if types.len() != 1 || types[0].as_bytes() != b"application/octet-stream" {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Expected application/octet-stream"));
    }
    let lengths: Vec<_> = headers.get_all(header::CONTENT_LENGTH).iter().collect();
    if lengths.len() > 1 {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "Invalid Content-Length"));
    }
    if let Some(value) = lengths.first() {
        let value = value
            .to_str()
            .ok()
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Invalid Content-Length"))?;
        if value > MAX_BODY_BYTES {
            return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "Package archive exceeds its wire limit"));
        }
    }
    Ok(())
}

fn descriptor_value(descriptor: &InstallationDescriptor) -> Value {
    let plugin = &descriptor.manifest().plugin;
    json!({"id": plugin.id.to_string(), "version": plugin.version.to_string(), "archive_sha256": descriptor.digest().to_string(), "wire_bytes": descriptor.wire_bytes()})
}

fn operation_value(operation: &InstallationOperation, url: &str) -> Value {
    let (stage, error) = match operation.stage {
        OperationStage::Pending => ("pending", None),
        OperationStage::AwaitingDurability => ("awaiting_durability", None),
        OperationStage::Complete => ("complete", None),
        OperationStage::Error(error) => (
            "error",
            Some(match error {
                OperationError::Source => "source",
                OperationError::Validation => "validation",
                OperationError::Publication => "publication",
                OperationError::Durability => "durability",
                OperationError::Conflict => "conflict",
                OperationError::Interrupted => "interrupted",
            }),
        ),
    };
    json!({"operation_id": operation.id.as_str(), "url": url, "stage": stage, "error": error, "revision": operation.revision,
        "package": descriptor_value(&operation.descriptor), "created_at": payloads::timestamp(operation.created_at), "updated_at": payloads::timestamp(operation.updated_at)})
}

fn fault(error: &ServiceError, source: Fault) -> ApiError {
    // Only classify source faults when the original error was a staging read;
    // publication/commit errors may mean durable bytes or rows already exist.
    if let ServiceError::Internal(original) = error {
        let store = original.downcast_ref::<StoreFailure>();
        if let Some(failure) = store
            && (failure.visibility() != PublicationVisibility::NotAttempted
                || failure.receipt().is_some()
                || failure.cleanup() != CleanupStatus::Complete)
        {
            return ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Package publication requires status inspection or host reconciliation");
        }
        let kind = store.map(StoreFailure::error).or_else(|| original.downcast_ref::<StoreError>().copied());
        if let Some(kind) = kind {
            return match kind {
                StoreError::Read if source == Fault::Quota => {
                    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "Package archive exceeds its wire limit")
                }
                StoreError::Read if source == Fault::Deadline => {
                    ApiError::new(StatusCode::REQUEST_TIMEOUT, "Package upload deadline expired")
                }
                StoreError::Read => ApiError::new(StatusCode::BAD_REQUEST, "Package upload could not be read"),
                StoreError::WireLimit | StoreError::Archive(ArchiveError::WireLimit) => {
                    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "Package archive exceeds its wire limit")
                }
                StoreError::Deadline | StoreError::Cancelled => {
                    ApiError::new(StatusCode::REQUEST_TIMEOUT, "Package operation interrupted; inspect its status")
                }
                StoreError::Archive(_) => ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid canonical package archive"),
                _ => ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Package store unavailable; host reconciliation may be required"),
            };
        }
    }
    match error {
        ServiceError::Forbidden => ApiError::forbidden("Active site administrator required"),
        ServiceError::Conflict("package operation admission" | "server shutdown") => {
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Package operation admission unavailable")
        }
        ServiceError::Conflict(_) => ApiError::new(StatusCode::CONFLICT, "Package operation conflicts with recorded or active work"),
        ServiceError::Invalid(_) => ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid package operation"),
        _ => ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Package operation failed; inspect its status"),
    }
}

/// `StreamReader` skips empty chunks internally. Force a yield between frames so
/// an always-ready empty-frame stream cannot monopolize one poll and starve the
/// outer cancellation/deadline checks. This stays demand-driven, without a pump.
fn cooperative_frames<S: Stream>(stream: S) -> impl Stream<Item = S::Item> {
    stream.then(|frame| async move {
        tokio::task::yield_now().await;
        frame
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Fault {
    None,
    Quota,
    Deadline,
    Cancelled,
}
impl Fault {
    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Quota,
            2 => Self::Deadline,
            3 => Self::Cancelled,
            _ => Self::None,
        }
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Deadlines/cancellation are independently polled while the bridge is awaiting
/// source bytes. `StageControl` alone cannot wake a blocked asynchronous body.
struct BoundedSource<R> {
    reader: Pin<Box<R>>,
    deadline: Pin<Box<Sleep>>,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
    fault: Arc<AtomicU8>,
    remaining: u64,
}
impl<R> BoundedSource<R> {
    fn new(reader: R, cancel: CancellationToken, deadline: Instant, quota: u64, fault: Arc<AtomicU8>) -> Self {
        Self {
            reader: Box::pin(reader),
            deadline: Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(deadline))),
            cancelled: Box::pin(cancel.cancelled_owned()),
            fault,
            remaining: quota,
        }
    }
    fn failed(&self, fault: Fault, kind: io::ErrorKind) -> Poll<io::Result<()>> {
        let _ = self.fault.compare_exchange(Fault::None as u8, fault as u8, Ordering::Relaxed, Ordering::Relaxed);
        Poll::Ready(Err(io::Error::new(kind, "package source interrupted")))
    }
}
impl<R: AsyncRead> AsyncRead for BoundedSource<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, output: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if this.cancelled.as_mut().poll(cx).is_ready() {
            return this.failed(Fault::Cancelled, io::ErrorKind::ConnectionAborted);
        }
        if this.deadline.as_mut().poll(cx).is_ready() {
            return this.failed(Fault::Deadline, io::ErrorKind::TimedOut);
        }
        // Read at most one extra byte to distinguish exact-size EOF from overflow.
        let limit = output.remaining().min(usize::try_from(this.remaining.saturating_add(1)).unwrap_or(usize::MAX));
        let mut part = ReadBuf::new(output.initialize_unfilled_to(limit));
        match this.reader.as_mut().poll_read(cx, &mut part) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let bytes = part.filled().len();
                if bytes as u64 > this.remaining {
                    return this.failed(Fault::Quota, io::ErrorKind::FileTooLarge);
                }
                this.remaining -= bytes as u64;
                output.advance(bytes);
                Poll::Ready(Ok(()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tokio::io::AsyncReadExt;

    #[test]
    fn source_faults_classify_staging_errors_without_masking_other_failures() {
        for (kind, source, expected) in [
            (StoreError::Read, Fault::Quota, StatusCode::PAYLOAD_TOO_LARGE),
            (StoreError::Read, Fault::Deadline, StatusCode::REQUEST_TIMEOUT),
            (StoreError::Read, Fault::None, StatusCode::BAD_REQUEST),
            (StoreError::WireLimit, Fault::None, StatusCode::PAYLOAD_TOO_LARGE),
            (StoreError::Deadline, Fault::None, StatusCode::REQUEST_TIMEOUT),
            (StoreError::Publish, Fault::Quota, StatusCode::SERVICE_UNAVAILABLE),
        ] {
            assert_eq!(fault(&ServiceError::internal(kind), source).status, expected);
        }
        for source in [Fault::Quota, Fault::Deadline] {
            let conflict = fault(&ServiceError::Conflict("private descriptor details"), source);
            assert_eq!(conflict.status, StatusCode::CONFLICT);
            assert!(!conflict.message.contains("private descriptor details"));
            let internal = fault(&ServiceError::internal(io::Error::other("private host path")), source);
            assert_eq!(internal.status, StatusCode::INTERNAL_SERVER_ERROR);
            assert!(!internal.message.contains("private host path"));
        }
    }

    #[tokio::test]
    async fn forever_ready_empty_frames_yield_before_cancellation_is_repolled() {
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = Arc::clone(&polls);
        let empty = futures_util::stream::repeat_with(move || {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok::<_, io::Error>(axum::body::Bytes::new())
        });
        let cancel = CancellationToken::new();
        let fault = Arc::new(AtomicU8::new(0));
        let mut reader = BoundedSource::new(
            StreamReader::new(cooperative_frames(empty)),
            cancel.clone(),
            Instant::now() + TIMEOUT,
            3,
            Arc::clone(&fault),
        );
        let mut storage = [0];
        let mut output = ReadBuf::new(&mut storage);
        let mut context = Context::from_waker(futures_util::task::noop_waker_ref());
        assert!(Pin::new(&mut reader).poll_read(&mut context, &mut output).is_pending());
        assert_eq!(polls.load(Ordering::Relaxed), 1);
        cancel.cancel();
        assert!(
            matches!(Pin::new(&mut reader).poll_read(&mut context, &mut output), Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::ConnectionAborted)
        );
        assert_eq!(polls.load(Ordering::Relaxed), 1);
        assert_eq!(fault.load(Ordering::Relaxed), Fault::Cancelled as u8);
    }

    #[tokio::test]
    async fn exact_source_quota_accepts_eof_but_rejects_an_extra_byte() {
        for (input, succeeds) in [(b"abc".as_slice(), true), (b"abcd".as_slice(), false)] {
            let fault = Arc::new(AtomicU8::new(0));
            let mut reader = BoundedSource::new(input, CancellationToken::new(), Instant::now() + TIMEOUT, 3, Arc::clone(&fault));
            let mut bytes = Vec::new();
            assert_eq!(reader.read_to_end(&mut bytes).await.is_ok(), succeeds);
            assert_eq!(fault.load(Ordering::Relaxed), if succeeds { Fault::None as u8 } else { Fault::Quota as u8 });
        }
    }

    struct Stalled(Option<tokio::sync::oneshot::Sender<()>>);
    impl AsyncRead for Stalled {
        fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, _: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            if let Some(started) = self.0.take() {
                let _ = started.send(());
            }
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn dropping_waiter_wakes_a_blocked_sync_bridge_without_source_progress() {
        let cancel = CancellationToken::new();
        let waiter = CancelOnDrop(cancel.clone());
        let (started, polled) = tokio::sync::oneshot::channel();
        let fault = Arc::new(AtomicU8::new(0));
        let source = BoundedSource::new(Stalled(Some(started)), cancel, Instant::now() + TIMEOUT, 3, Arc::clone(&fault));
        let mut bridge = SyncIoBridge::new(source);
        let worker = tokio::task::spawn_blocking(move || Read::read(&mut bridge, &mut [0]));
        polled.await.unwrap();
        drop(waiter);
        let error = tokio::time::timeout(Duration::from_secs(2), worker).await.unwrap().unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert_eq!(fault.load(Ordering::Relaxed), Fault::Cancelled as u8);
    }

    #[tokio::test]
    async fn absolute_deadline_wakes_a_stalled_source() {
        let fault = Arc::new(AtomicU8::new(0));
        let mut source =
            BoundedSource::new(Stalled(None), CancellationToken::new(), Instant::now() + Duration::from_millis(10), 3, Arc::clone(&fault));
        let result = tokio::time::timeout(Duration::from_secs(2), source.read(&mut [0])).await.unwrap().unwrap_err();
        assert_eq!(result.kind(), io::ErrorKind::TimedOut);
        assert_eq!(fault.load(Ordering::Relaxed), Fault::Deadline as u8);
    }

    #[test]
    fn workflow_credentials_have_no_package_administration_route() {
        assert!(super::super::auth::task_route(Put, PATH).is_none());
        assert!(super::super::auth::task_route(Get, PATH).is_none());
    }

    #[test]
    fn headers_reject_unsupported_or_oversized_transport_without_reading() {
        let mut headers = HeaderMap::new();
        assert_eq!(check_headers(&headers).unwrap_err().status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
        assert!(check_headers(&headers).is_ok());
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_str(&MAX_BODY_BYTES.to_string()).unwrap());
        assert!(check_headers(&headers).is_ok());
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_str(&(MAX_BODY_BYTES + 1).to_string()).unwrap());
        assert_eq!(check_headers(&headers).unwrap_err().status, StatusCode::PAYLOAD_TOO_LARGE);
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        headers.append(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        assert_eq!(check_headers(&headers).unwrap_err().status, StatusCode::BAD_REQUEST);
        headers.remove(header::CONTENT_LENGTH);
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        assert_eq!(check_headers(&headers).unwrap_err().status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
