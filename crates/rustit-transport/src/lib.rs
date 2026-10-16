//! # rustit-transport
//!
//! The Git "smart" protocol, expressed without any web framework or SSH library types so
//! both the HTTP routes and the built-in SSH server share it.
//!
//! The backend, [`ProcessTransport`], runs `git upload-pack` / `git receive-pack` with a
//! scrubbed environment. That gives full protocol v2, shallow clone, partial clone and
//! push-option support from Git's reference implementation. Two modes:
//!
//! * **Stateless** (HTTP): [`ProcessTransport::advertise_refs`] and
//!   [`ProcessTransport::serve`] run `--stateless-rpc` request/response exchanges.
//! * **Interactive** (SSH): [`ProcessTransport::serve_interactive`] runs one long
//!   bidirectional conversation and reports git's exit status.
//!
//! Data is streamed in both modes; pack data is never buffered in memory. The
//! client-controlled `GIT_PROTOCOL` value is filtered by [`is_safe_protocol_header`] in
//! both.

pub mod pkt_line;
pub mod process;
pub mod service;

pub use process::{ProcessTransport, ServiceSession, TransportError, is_safe_protocol_header};
pub use service::Service;
