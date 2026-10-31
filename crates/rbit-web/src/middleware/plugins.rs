//! Runtime Actions admission, response-body ownership and UI revision metadata.

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::AppState;

/// Recognizes workload credentials without decoding unbounded attacker-controlled input.
fn task_credential(request: &Request) -> bool {
    let Some(value) = request.headers().get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) else { return false };
    if value.len() > 4096 {
        return false;
    }
    let Some((scheme, value)) = value.trim().split_once(' ') else { return false };
    if scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("token") {
        return rbit_core::actions::looks_like_task_token(value.trim());
    }
    if !scheme.eq_ignore_ascii_case("basic") {
        return false;
    }
    let Ok(bytes) = STANDARD.decode(value.trim()) else { return false };
    let Ok(decoded) = std::str::from_utf8(&bytes) else { return false };
    let Some((login, password)) = decoded.split_once(':') else { return false };
    rbit_core::actions::looks_like_task_token(login) || rbit_core::actions::looks_like_task_token(password)
}

fn with_revision(mut response: Response, revision: u64) -> Response {
    if let Ok(value) = revision.to_string().parse() {
        response.headers_mut().insert("rbit-plugin-revision", value);
    }
    response
}

fn refused(task: bool, request: &Request, revision: u64) -> Response {
    // These two-segment paths otherwise match the host's GET-only repository route.
    // Preserve its method-not-allowed behavior while optional routes are absent.
    if !task
        && !matches!(*request.method(), axum::http::Method::GET | axum::http::Method::HEAD)
        && matches!(request.uri().path(), "/admin/runners" | "/settings/actions")
    {
        return with_revision(
            (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "GET,HEAD"), (header::CACHE_CONTROL, "no-store")]).into_response(),
            revision,
        );
    }
    let status = if task { StatusCode::UNAUTHORIZED } else { StatusCode::NOT_FOUND };
    with_revision((status, [(header::CACHE_CONTROL, "no-store"), (header::X_CONTENT_TYPE_OPTIONS, "nosniff")]).into_response(), revision)
}

/// A plugin request owns admission until its response body completes or is dropped.
pub async fn plugin_admission(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Padding cannot bypass classification and then be trimmed by an authenticator.
    if request.headers().get(header::AUTHORIZATION).is_some_and(|value| value.as_bytes().len() > 4096) {
        return with_revision(
            (StatusCode::UNAUTHORIZED, [(header::CACHE_CONTROL, "no-store")]).into_response(),
            state.services.plugin_revision(),
        );
    }
    let task = task_credential(&request);
    #[cfg(feature = "actions")]
    let dedicated =
        request.extensions().get::<axum::extract::MatchedPath>().is_some_and(|path| crate::routes::is_actions_route(path.as_str()));

    #[cfg(feature = "actions")]
    let admitted = if dedicated || task {
        let Some(actions) = state.services.actions.snapshot() else { return refused(task, &request, state.services.plugin_revision()) };
        let Ok(lease) = actions.admit() else { return refused(task, &request, state.services.plugin_revision()) };
        Some((lease, actions.cancellation_token()))
    } else {
        None
    };
    #[cfg(not(feature = "actions"))]
    if task {
        return refused(true, &request, state.services.plugin_revision());
    }

    #[cfg(feature = "actions")]
    let request = if let Some((_, cancellation)) = &admitted {
        let (parts, body) = request.into_parts();
        Request::from_parts(
            parts,
            axum::body::Body::new(CancellableBody {
                body,
                finished: false,
                cancellation: Box::pin(cancellation.clone().cancelled_owned()),
            }),
        )
    } else {
        request
    };

    let revision = state.services.plugin_revision();
    let read = matches!(*request.method(), axum::http::Method::GET | axum::http::Method::HEAD);
    let target = request.uri().clone();
    let mut response = next.run(request).await;
    let current_revision = state.services.plugin_revision();
    let html = response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
    if read && html && current_revision != revision {
        // Rendering and shell metadata must describe the same publication. Retry a
        // navigation that crossed a lifecycle transition instead of caching mixed UI.
        let mut refresh = axum::response::Redirect::temporary(&target.to_string()).into_response();
        refresh.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
        refresh.headers_mut().insert("hx-refresh", axum::http::HeaderValue::from_static("true"));
        return with_revision(refresh, current_revision);
    }
    response = with_revision(response, revision);
    #[cfg(feature = "actions")]
    if let Some((lease, cancellation)) = admitted {
        let (parts, body) = response.into_parts();
        response = Response::from_parts(parts, supervised_plugin_body(body, lease, cancellation));
    }
    response
}

/// Drive admitted output independently of Hyper's polling. A stopped peer must not
/// prevent cancellation or keep native feature resources alive after disposal.
#[cfg(feature = "actions")]
fn supervised_plugin_body(
    mut body: axum::body::Body,
    lease: rbit_plugin_runtime::OperationLease,
    cancellation: tokio_util::sync::CancellationToken,
) -> axum::body::Body {
    use http_body::Body as _;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let hint = body.size_hint();
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let failed = Arc::new(AtomicBool::new(false));
    let failure = Arc::clone(&failed);
    let stopped = cancellation.clone();
    tokio::spawn(async move {
        let cancelled = loop {
            let frame = tokio::select! {
                () = stopped.cancelled() => break true,
                frame = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)) => frame,
            };
            let Some(frame) = frame else { break false };
            let last = frame.is_err();
            tokio::select! {
                () = stopped.cancelled() => break true,
                sent = sender.send(frame) => if sent.is_err() { break false; },
            }
            if last {
                break false;
            }
        };
        // Releasing operation authority must follow dropping its source, which may
        // own a file reader, Git stream or plugin-specific native resources.
        drop(body);
        drop(lease);
        if cancelled {
            failure.store(true, Ordering::Release);
        }
    });
    axum::body::Body::new(PluginResponseBody { receiver, cancellation, failed, hint, finished: false })
}

#[cfg(feature = "actions")]
#[derive(Debug)]
struct PluginResponseBody {
    receiver: tokio::sync::mpsc::Receiver<Result<http_body::Frame<axum::body::Bytes>, axum::Error>>,
    cancellation: tokio_util::sync::CancellationToken,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    hint: http_body::SizeHint,
    finished: bool,
}

#[cfg(feature = "actions")]
impl Drop for PluginResponseBody {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[cfg(feature = "actions")]
impl http_body::Body for PluginResponseBody {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.finished {
            return std::task::Poll::Ready(None);
        }
        if self.cancellation.is_cancelled() || self.failed.load(std::sync::atomic::Ordering::Acquire) {
            self.finished = true;
            self.receiver.close();
            while self.receiver.try_recv().is_ok() {}
            return std::task::Poll::Ready(Some(Err(axum::Error::new(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "plugin disabled",
            )))));
        }
        match self.receiver.poll_recv(cx) {
            std::task::Poll::Ready(None) => {
                self.finished = true;
                // Cancellation may race the first flag read and channel closure.
                if self.failed.load(std::sync::atomic::Ordering::Acquire) {
                    std::task::Poll::Ready(Some(Err(axum::Error::new(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "plugin disabled",
                    )))))
                } else {
                    std::task::Poll::Ready(None)
                }
            }
            std::task::Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
                    let lower = self.hint.lower().saturating_sub(bytes);
                    let upper = self.hint.upper().map(|upper| upper.saturating_sub(bytes));
                    self.hint = http_body::SizeHint::new();
                    self.hint.set_lower(lower);
                    if let Some(upper) = upper {
                        self.hint.set_upper(upper);
                    }
                }
                std::task::Poll::Ready(Some(Ok(frame)))
            }
            std::task::Poll::Ready(Some(Err(error))) => {
                self.finished = true;
                std::task::Poll::Ready(Some(Err(error)))
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.finished
    }
    fn size_hint(&self) -> http_body::SizeHint {
        if self.finished { http_body::SizeHint::with_exact(0) } else { self.hint }
    }
}

#[cfg(feature = "actions")]
struct CancellableBody {
    body: axum::body::Body,
    cancellation: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
    finished: bool,
}

#[cfg(feature = "actions")]
impl std::fmt::Debug for CancellableBody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CancellableBody").field("finished", &self.finished).finish_non_exhaustive()
    }
}

#[cfg(feature = "actions")]
impl http_body::Body for CancellableBody {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.finished {
            return std::task::Poll::Ready(None);
        }
        if self.cancellation.as_mut().poll(cx).is_ready() {
            self.finished = true;
            self.body = axum::body::Body::empty();
            return std::task::Poll::Ready(Some(Err(axum::Error::new(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "plugin disabled",
            )))));
        }
        let result = std::pin::Pin::new(&mut self.body).poll_frame(cx);
        if matches!(result, std::task::Poll::Ready(None | Some(Err(_)))) {
            self.finished = true;
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.finished || self.body.is_end_stream()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_credentials_are_bounded_and_detected_in_both_basic_fields() {
        let token = format!("rbat_{}", "a".repeat(43));
        for authorization in [
            format!("Bearer {token}"),
            format!("token {token}"),
            format!("Basic {}", STANDARD.encode(format!("x-access-token:{token}"))),
            format!("Basic {}", STANDARD.encode(format!("{token}:"))),
        ] {
            let request = Request::builder().header(header::AUTHORIZATION, authorization).body(axum::body::Body::empty()).unwrap();
            assert!(task_credential(&request));
        }
        for authorization in ["Basic invalid".to_owned(), "Bearer rbit_personal".to_owned(), format!("Basic {}", "a".repeat(5000))] {
            let request = Request::builder().header(header::AUTHORIZATION, authorization).body(axum::body::Body::empty()).unwrap();
            assert!(!task_credential(&request));
        }
    }

    #[cfg(feature = "actions")]
    fn admitted(
        body: axum::body::Body,
        scope: &rbit_plugin_runtime::PluginScope,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> axum::body::Body {
        supervised_plugin_body(body, scope.try_acquire(scope.generation()).unwrap(), cancellation)
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn response_body_holds_admission_until_end_or_drop() {
        use std::time::Duration;
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let body = admitted(
            axum::body::Body::from_stream(futures_util::stream::pending::<Result<axum::body::Bytes, std::io::Error>>()),
            &scope,
            tokio_util::sync::CancellationToken::new(),
        );
        assert_eq!(scope.in_flight(), 1);
        assert!(scope.dispose(Duration::ZERO).await.is_err());
        drop(body);
        scope.dispose(Duration::from_secs(1)).await.unwrap();

        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let body = admitted(axum::body::Body::from("complete"), &scope, tokio_util::sync::CancellationToken::new());
        assert_eq!(axum::body::to_bytes(body, 64).await.unwrap(), "complete");
        scope.dispose(Duration::from_secs(1)).await.unwrap();
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn cancellation_interrupts_a_pending_stream_and_releases_its_lease() {
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let cancellation = tokio_util::sync::CancellationToken::new();
        let body = axum::body::Body::from_stream(futures_util::stream::pending::<Result<axum::body::Bytes, std::io::Error>>());
        let body = admitted(body, &scope, cancellation.clone());
        cancellation.cancel();
        assert!(axum::body::to_bytes(body, 64).await.is_err());
        scope.dispose(std::time::Duration::from_secs(1)).await.unwrap();
        assert_eq!(scope.in_flight(), 0);
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn quiesce_wakes_a_stalled_request_reader_before_its_handler_lease_is_released() {
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let lease = scope.try_acquire(scope.generation()).unwrap();
        let cancellation = scope.cancellation_token();
        let incoming = axum::body::Body::new(CancellableBody {
            body: axum::body::Body::from_stream(futures_util::stream::pending::<Result<axum::body::Bytes, std::io::Error>>()),
            finished: false,
            cancellation: Box::pin(cancellation.cancelled_owned()),
        });
        let handler = tokio::spawn(async move {
            let result = axum::body::to_bytes(incoming, 64).await;
            drop(lease);
            result
        });
        assert_eq!(scope.in_flight(), 1);
        scope.quiesce();
        assert!(tokio::time::timeout(std::time::Duration::from_secs(1), handler).await.unwrap().unwrap().is_err());
        scope.dispose(std::time::Duration::ZERO).await.unwrap();
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn disable_drains_an_unread_response_even_when_its_bounded_channel_is_full() {
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let source = axum::body::Body::from_stream(futures_util::stream::repeat_with(|| {
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"bounded frame"))
        }));
        let response = admitted(source, &scope, scope.cancellation_token());
        // Leave the outward body entirely unpolled, as a peer under backpressure does.
        tokio::task::yield_now().await;
        assert_eq!(scope.in_flight(), 1);
        scope.dispose(std::time::Duration::from_secs(1)).await.unwrap();
        assert_eq!(scope.in_flight(), 0);
        assert!(axum::body::to_bytes(response, 1024).await.is_err());
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn disable_discards_a_finished_but_unread_secret_bearing_response() {
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let response = admitted(axum::body::Body::from("private job specification"), &scope, scope.cancellation_token());
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while scope.in_flight() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // The producer has finished, but the bounded receiver still holds its frame.
        scope.dispose(std::time::Duration::ZERO).await.unwrap();
        assert!(axum::body::to_bytes(response, 1024).await.is_err());
    }

    #[cfg(feature = "actions")]
    #[tokio::test]
    async fn response_supervision_preserves_data_and_trailer_frames() {
        use http_body::Body as _;
        struct Frames(std::collections::VecDeque<http_body::Frame<axum::body::Bytes>>);
        impl http_body::Body for Frames {
            type Data = axum::body::Bytes;
            type Error = std::io::Error;
            fn poll_frame(
                mut self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
                std::task::Poll::Ready(self.0.pop_front().map(Ok))
            }
        }
        let mut trailers = axum::http::HeaderMap::new();
        trailers.insert("x-integrity", axum::http::HeaderValue::from_static("verified"));
        let source = axum::body::Body::new(Frames(std::collections::VecDeque::from([
            http_body::Frame::data(axum::body::Bytes::from_static(b"payload")),
            http_body::Frame::trailers(trailers),
        ])));
        let scope = rbit_plugin_runtime::PluginScope::new(rbit_plugin_api::Generation::INITIAL);
        let mut body = admitted(source, &scope, scope.cancellation_token());
        let frame = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await.unwrap().unwrap();
        assert_eq!(frame.data_ref().unwrap(), "payload");
        let frame = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await.unwrap().unwrap();
        assert_eq!(frame.trailers_ref().unwrap()["x-integrity"], "verified");
        assert!(std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await.is_none());
        scope.dispose(std::time::Duration::from_secs(1)).await.unwrap();
    }
}
