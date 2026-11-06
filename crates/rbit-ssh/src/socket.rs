//! Cancelling a connection waiter also interrupts russh's internally spawned I/O.

use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

pub(crate) struct CancellableSocket {
    stream: TcpStream,
    read_stopped: Pin<Box<WaitForCancellationFutureOwned>>,
    write_stopped: Pin<Box<WaitForCancellationFutureOwned>>,
}

impl CancellableSocket {
    pub(crate) fn new(stream: TcpStream, stop: CancellationToken) -> Self {
        Self { stream, read_stopped: Box::pin(stop.clone().cancelled_owned()), write_stopped: Box::pin(stop.cancelled_owned()) }
    }
    fn stopped(&mut self, cx: &mut Context<'_>, write: bool) -> bool {
        if write { self.write_stopped.as_mut().poll(cx).is_ready() } else { self.read_stopped.as_mut().poll(cx).is_ready() }
    }
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, "SSH connection stopped")
}

impl AsyncRead for CancellableSocket {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.stopped(cx, false) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buffer)
    }
}
impl AsyncWrite for CancellableSocket {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &[u8]) -> Poll<io::Result<usize>> {
        if self.stopped(cx, true) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut self.stream).poll_write(cx, buffer)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.stopped(cx, true) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.stopped(cx, true) {
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

pub(crate) struct CloseOnDrop(pub(crate) CancellationToken);
impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Wake, Waker},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn socket_forwards_live_bytes_then_cancellation_wakes_blocked_io() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let stop = CancellationToken::new();
        let mut socket = CancellableSocket::new(server, stop.clone());
        client.write_all(b"hello").await.unwrap();
        let mut bytes = [0; 5];
        socket.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"hello");
        let read = tokio::spawn(async move { socket.read_u8().await });
        drop(CloseOnDrop(stop));
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), read).await.unwrap().unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ConnectionAborted);
    }

    #[tokio::test]
    async fn read_and_write_pollers_keep_independent_cancellation_wakers() {
        struct Wakes(AtomicUsize);
        impl Wake for Wakes {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let stop = CancellationToken::new();
        let mut socket = CancellableSocket::new(server, stop.clone());
        let read = Arc::new(Wakes(AtomicUsize::new(0)));
        let write = Arc::new(Wakes(AtomicUsize::new(0)));
        let read_waker = Waker::from(Arc::clone(&read));
        let write_waker = Waker::from(Arc::clone(&write));
        let mut bytes = [0];
        let mut buffer = ReadBuf::new(&mut bytes);
        assert!(Pin::new(&mut socket).poll_read(&mut Context::from_waker(&read_waker), &mut buffer).is_pending());
        let _ = Pin::new(&mut socket).poll_flush(&mut Context::from_waker(&write_waker));
        stop.cancel();
        assert!(read.0.load(Ordering::SeqCst) > 0);
        assert!(write.0.load(Ordering::SeqCst) > 0);
    }
}
