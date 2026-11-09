//! Notification email policy: who is mailed about what, what each message says, how it
//! threads, one-click unsubscribe tokens and daily or weekly digests. Templates, the
//! transport, throttling, keys and SQL stay with composition and storage behind ports.
pub mod activity;
pub mod delivery;
pub mod digest;
pub mod policy;
pub mod port;
pub mod records;
pub mod unsubscribe;
