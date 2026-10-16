//! # rustit-transport
//!
//! The Git "smart" protocol as used over HTTP, expressed without any web framework types
//! so it can be reused by an SSH server later.
//!
//! The current backend, [`ProcessTransport`], streams to `git upload-pack` /
//! `git receive-pack` in `--stateless-rpc` mode. That gives full protocol v2, shallow
//! clone, partial clone and push-option support from Git's reference implementation.
//! Request and response bodies are streamed; pack data is never buffered in memory.

pub mod pkt_line;
pub mod process;
pub mod service;

pub use process::{ProcessTransport, ServiceSession, TransportError};
pub use service::Service;
