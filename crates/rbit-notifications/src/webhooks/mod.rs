//! Webhook policy: event envelopes, the signed wire contract, management limits and the
//! retry decision. Hook storage, the shared JSON object shapes, queueing, the worker, the
//! HTTP client and its outbound address guard stay with composition for now.
pub mod payloads;
pub mod policy;
pub mod protocol;
