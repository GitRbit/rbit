//! Smart HTTP decisions, without any web framework types.
//!
//! The HTTP routes own extraction and rendering only; what a Git client is told, and when,
//! is decided here:
//!
//! * [`credentials`]: reading `Authorization` (Basic login and password, or a workflow
//!   task token sent as `Bearer`/`token`) and the challenge that asks Git for them.
//! * [`access`]: the order of the repository, write and pull-mirror checks behind the
//!   [`access::RepositoryGate`] port, and which refusal each outcome becomes (a
//!   credential challenge for anonymous callers, never a definitive answer).
//! * Protocol framing: the `Git-Protocol` header carrying protocol v2, gzip request
//!   bodies, caching and the refusal of the dumb protocol. The `ERR` packet
//!   ([`crate::pkt_line::error`]) and refused-push answers ([`crate::refused_push`]) are
//!   shared with SSH.
//!
//! Every value here is byte-for-byte what Git clients already receive; changing one is a
//! protocol change.

pub mod access;
pub mod credentials;

/// The request header carrying the client's protocol version (`version=2`); its value is
/// passed to git only after [`crate::is_safe_protocol_header`].
pub const GIT_PROTOCOL_HEADER: &str = "git-protocol";

/// `Cache-Control` of every advertisement and result: Git responses are never cached.
pub const NO_CACHE: &str = "no-cache";

/// The answer to `info/refs` without a smart `service` parameter.
pub const DUMB_PROTOCOL_REFUSED: &str = "the dumb HTTP protocol is not supported; please upgrade git";

/// Whether a `Content-Encoding` value means the body is gzip-compressed, as Git sends
/// large negotiation requests.
pub fn is_gzip(content_encoding: &[u8]) -> bool {
    content_encoding.eq_ignore_ascii_case(b"gzip")
}

/// Most bytes a gzip-compressed fetch request may expand to. Its wants and haves are a few
/// megabytes even for very large repositories, while a request of kilobytes could expand
/// to gigabytes for the server to inflate and git to parse, before anyone signed in.
pub const MAX_INFLATED_REQUEST_BYTES: u64 = 64 << 20;

/// A reader that fails (`InvalidData`) once it would give more than its limit, so an
/// oversized request ends the exchange rather than being cut short silently.
#[derive(Debug)]
pub struct Capped<R> {
    inner: R,
    left: u64,
}

impl<R> Capped<R> {
    pub const fn new(inner: R, limit: u64) -> Self {
        Self { inner, left: limit }
    }
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Capped<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        std::task::ready!(std::pin::Pin::new(&mut self.inner).poll_read(cx, buf))?;
        let read = u64::try_from(buf.filled().len() - before).unwrap_or(u64::MAX);
        if read > self.left {
            buf.set_filled(before);
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the request is larger than the server accepts once decompressed",
            )));
        }
        self.left -= read;
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests;
