//! Webhook policy: event envelopes, hook selection and queueing, the signed wire
//! contract, management limits, the retry decision and the worker's batches and settling.
//! Hook storage, the shared JSON object shapes, task spawning and shutdown, the HTTP
//! client and its outbound address guard stay with composition.
pub mod payloads;
pub mod policy;
pub mod protocol;
pub mod queue;
pub mod worker;
