//! # rustit-transport
//!
//! The Git "smart" protocol, expressed without any web framework or SSH library types so
//! both the HTTP routes and the built-in SSH server share it.
//!
//! The backend, [`ProcessTransport`], runs `git upload-pack` / `git receive-pack` with a
//! scrubbed environment. That gives full protocol v2, shallow clone, partial clone and
//! push-option support from Git's reference implementation. Two modes:
//!
//! * **Stateless** (HTTP, and SSH pushes): [`ProcessTransport::advertise_refs`] and
//!   [`ProcessTransport::serve`] run `--stateless-rpc` request/response exchanges; SSH
//!   pushes use [`ProcessTransport::advertisement`] then [`ProcessTransport::receive_pack`],
//!   the same two steps.
//! * **Interactive** (SSH fetches): [`ProcessTransport::serve_interactive`] runs one long
//!   bidirectional conversation and reports git's exit status.
//!
//! Pushes pass the **push gate** in between: [`receive`] reads a push's command list before
//! git does, so branch protection can refuse it, and decides the [`ReceiveOptions`] git
//! runs with.
//!
//! Data is streamed in all modes; pack data is never buffered in memory. The
//! client-controlled `GIT_PROTOCOL` value is filtered by [`is_safe_protocol_header`] in
//! both.

pub mod pkt_line;
pub mod process;
pub mod receive;
pub mod service;

pub use process::{ProcessTransport, ReceiveOptions, ServiceSession, TransportError, is_safe_protocol_header};
pub use service::Service;
