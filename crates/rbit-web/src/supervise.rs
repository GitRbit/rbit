//! Long-running streams that cannot stall forever: Git smart-HTTP exchanges and archive
//! downloads.
//!
//! Hyper only polls a response body when the client can take more bytes, so a client that
//! stops reading would keep whatever the body owns (a git process, a concurrency slot)
//! alive indefinitely. [`supervised_body`] therefore reads the source in its own task and
//! hands chunks to the body through a one-chunk channel. The task gives up when nothing
//! has moved for the idle timeout or the exchange outlives its time limit, dropping the
//! source (which kills git) and the guard (which frees the slot); the body then ends with
//! an error, so a truncated download is never passed off as a complete one.
//!
//! [`Activity`] is the shared record of when bytes last moved. [`Watched`] wraps a request
//! body so uploads count as activity too, and stops its reader on the same limits.

use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, ready},
    time::Duration,
};

use axum::body::{Body, Bytes};
use futures_util::StreamExt;
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc,
    time::{Instant, Sleep},
};
use tokio_util::io::ReaderStream;

/// Bytes read from the source at a time.
const CHUNK: usize = 32 * 1024;

/// How long a stream may stall, and how long it may last in all.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StreamLimits {
    pub(crate) idle: Duration,
    pub(crate) total: Duration,
}

/// When an exchange must end, and when bytes last moved in either direction.
#[derive(Debug, Clone)]
pub(crate) struct Activity(Arc<ActivityInner>);

#[derive(Debug)]
struct ActivityInner {
    started: Instant,
    deadline: Instant,
    idle: Duration,
    /// Milliseconds after `started` that bytes last moved.
    last_ms: AtomicU64,
}

impl Activity {
    /// Starts the clock of an exchange.
    pub(crate) fn start(limits: StreamLimits) -> Self {
        let started = Instant::now();
        Self(Arc::new(ActivityInner { started, deadline: started + limits.total, idle: limits.idle, last_ms: AtomicU64::new(0) }))
    }

    /// Records that bytes moved just now.
    fn touch(&self) {
        let elapsed = u64::try_from(self.0.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.0.last_ms.fetch_max(elapsed, Ordering::Relaxed);
    }

    /// When the exchange expires unless bytes move first.
    fn expiry(&self) -> Instant {
        let last = self.0.started + Duration::from_millis(self.0.last_ms.load(Ordering::Relaxed));
        (last + self.0.idle).min(self.0.deadline)
    }

    /// An error once the exchange has stalled for too long or outlived its time limit.
    fn check(&self) -> io::Result<()> {
        let now = Instant::now();
        if now >= self.0.deadline {
            Err(io::Error::new(io::ErrorKind::TimedOut, "the transfer took too long and was stopped"))
        } else if now >= self.expiry() {
            Err(io::Error::new(io::ErrorKind::TimedOut, "the transfer stalled and was stopped"))
        } else {
            Ok(())
        }
    }

    /// Runs `future` until it finishes, or fails once the exchange expires.
    async fn bound<F: Future>(&self, future: F) -> io::Result<F::Output> {
        let mut future = std::pin::pin!(future);
        loop {
            tokio::select! {
                output = &mut future => return Ok(output),
                () = tokio::time::sleep_until(self.expiry()) => self.check()?,
            }
        }
    }
}

/// A response body streaming `source` under `activity`'s limits; `guard` (a concurrency
/// slot) is held until the source is finished or abandoned.
pub(crate) fn supervised_body<R, G>(source: R, guard: G, activity: Activity) -> Body
where
    R: AsyncRead + Send + 'static,
    G: Send + 'static,
{
    let (sender, receiver) = mpsc::channel::<io::Result<Bytes>>(1);
    let failed = Arc::new(AtomicBool::new(false));
    let failure = Arc::clone(&failed);
    tokio::spawn(async move {
        let _guard = guard;
        let mut chunks = std::pin::pin!(ReaderStream::with_capacity(source, CHUNK));
        let stopped = loop {
            let item = match activity.bound(chunks.next()).await {
                Ok(Some(item)) => item,
                Ok(None) => break None,
                Err(error) => break Some(error),
            };
            let last = item.is_err();
            if item.is_ok() {
                activity.touch();
            }
            match activity.bound(sender.send(item)).await {
                // The source failed (and said so to the client), or finished.
                Ok(Ok(())) if last => break None,
                Ok(Ok(())) => activity.touch(),
                // The client went away.
                Ok(Err(_)) => break None,
                Err(error) => break Some(error),
            }
        };
        if let Some(error) = stopped {
            tracing::debug!(%error, "stopped a response stream");
            failure.store(true, Ordering::Release);
        }
    });

    let stream = futures_util::stream::unfold((receiver, failed), |(mut receiver, failed)| async move {
        match receiver.recv().await {
            Some(item) => Some((item, (receiver, failed))),
            None if failed.swap(false, Ordering::AcqRel) => {
                Some((Err(io::Error::new(io::ErrorKind::TimedOut, "the transfer was stopped")), (receiver, failed)))
            }
            None => None,
        }
    });
    Body::from_stream(stream)
}

/// A request body whose bytes count as activity, and whose reads fail once the exchange
/// expires (so a client that stalls mid-upload cannot hold its reader forever).
#[derive(Debug)]
pub(crate) struct Watched<R> {
    inner: R,
    activity: Activity,
    timer: Pin<Box<Sleep>>,
}

impl<R> Watched<R> {
    pub(crate) fn new(inner: R, activity: Activity) -> Self {
        let timer = Box::pin(tokio::time::sleep_until(activity.expiry()));
        Self { inner, activity, timer }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Watched<R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        if let Poll::Ready(result) = Pin::new(&mut self.inner).poll_read(cx, buf) {
            if buf.filled().len() != before {
                self.activity.touch();
            }
            return Poll::Ready(result);
        }
        loop {
            let expiry = self.activity.expiry();
            if self.timer.deadline() != expiry {
                self.timer.as_mut().reset(expiry);
            }
            ready!(self.timer.as_mut().poll(cx));
            self.activity.check()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const SHORT: Duration = Duration::from_millis(200);

    fn limits(idle: Duration, total: Duration) -> StreamLimits {
        StreamLimits { idle, total }
    }

    /// Resolves once the guard handed to the body has been dropped.
    fn guard() -> (tokio::sync::oneshot::Sender<()>, tokio::sync::oneshot::Receiver<()>) {
        tokio::sync::oneshot::channel()
    }

    #[tokio::test]
    async fn a_finished_source_ends_the_body_cleanly() {
        let (held, released) = guard();
        let body = supervised_body(&b"hello"[..], held, Activity::start(limits(SHORT, Duration::from_secs(10))));
        let bytes = axum::body::to_bytes(body, 1024).await.unwrap();
        assert_eq!(&bytes[..], b"hello");
        released.await.unwrap_err();
    }

    #[tokio::test]
    async fn a_stalled_source_fails_the_body_and_frees_the_guard() {
        let (_writer, reader) = tokio::io::duplex(64);
        let (held, released) = guard();
        let body = supervised_body(reader, held, Activity::start(limits(SHORT, Duration::from_secs(10))));
        let result = tokio::time::timeout(Duration::from_secs(5), axum::body::to_bytes(body, 1024)).await.expect("stalled forever");
        assert!(result.is_err(), "a stalled stream must not end cleanly");
        released.await.unwrap_err();
    }

    #[tokio::test]
    async fn a_client_that_stops_reading_frees_the_guard() {
        let (mut writer, reader) = tokio::io::duplex(1024 * 1024);
        writer.write_all(&vec![b'x'; 512 * 1024]).await.unwrap();
        let (held, released) = guard();
        let body = supervised_body(reader, held, Activity::start(limits(SHORT, Duration::from_secs(10))));
        // The body is never polled, like a response to a client that does not read.
        tokio::time::timeout(Duration::from_secs(5), released).await.expect("the slot stayed taken").unwrap_err();
        drop(body);
    }

    #[tokio::test]
    async fn a_steady_trickle_is_cut_off_at_the_time_limit() {
        let (mut writer, reader) = tokio::io::duplex(64);
        let (held, released) = guard();
        let body = supervised_body(reader, held, Activity::start(limits(Duration::from_secs(5), Duration::from_millis(500))));
        let trickle = tokio::spawn(async move {
            while writer.write_all(b".").await.is_ok() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
        let result = tokio::time::timeout(Duration::from_secs(5), axum::body::to_bytes(body, 1024 * 1024)).await.expect("ran forever");
        assert!(result.is_err());
        released.await.unwrap_err();
        trickle.abort();
    }

    #[tokio::test]
    async fn uploads_keep_a_silent_response_alive() {
        let activity = Activity::start(limits(SHORT, Duration::from_secs(10)));
        let (mut client, upload) = tokio::io::duplex(64);
        let mut request = Watched::new(upload, activity.clone());
        let (mut git, response) = tokio::io::duplex(64);
        let body = supervised_body(response, (), activity);
        let reading = tokio::spawn(async move {
            let mut received = Vec::new();
            request.read_to_end(&mut received).await.map(|_| received.len())
        });
        // Five times the idle timeout of upload, while the response says nothing.
        for _ in 0..20 {
            client.write_all(b"pack").await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        drop(client);
        assert_eq!(reading.await.unwrap().unwrap(), 80);
        git.write_all(b"ok").await.unwrap();
        drop(git);
        assert_eq!(&axum::body::to_bytes(body, 1024).await.unwrap()[..], b"ok");
    }

    #[tokio::test]
    async fn a_stalled_upload_fails_its_reader() {
        let activity = Activity::start(limits(SHORT, Duration::from_secs(10)));
        let (_client, upload) = tokio::io::duplex(64);
        let mut request = Watched::new(upload, activity);
        let mut buffer = [0; 16];
        let result = tokio::time::timeout(Duration::from_secs(5), request.read(&mut buffer)).await.expect("waited forever");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
}
