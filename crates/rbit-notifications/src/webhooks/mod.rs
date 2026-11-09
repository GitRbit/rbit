//! Webhook delivery policy: the signed wire contract, management limits and the retry
//! decision. Hook storage, payloads, queueing, the HTTP client and its outbound address
//! guard stay with composition for now.
pub mod policy;
pub mod protocol;
