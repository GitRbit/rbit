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

#[cfg(test)]
mod tests;
