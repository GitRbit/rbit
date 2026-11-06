//! Session data contracts without runtime, crypto, or persistence dependencies.

pub use rbit_identity_models::sessions::{
    ActiveSession, ClientInfo, EpochSessionRepository, EpochSessionSnapshot, EpochSessionUserRepository, SessionRecord, SessionRepository,
    SessionUser, SessionUserRepository, StoredSession, session_id,
};

#[cfg(feature = "sessions")]
mod application;
#[cfg(feature = "sessions")]
pub use application::{SessionApplication, SessionClock, SessionError, SessionPolicy, SessionResult, SystemSessionClock};

#[cfg(feature = "sessions")]
pub mod effects;

#[cfg(feature = "sessions")]
mod fenced;
#[cfg(feature = "sessions")]
pub use fenced::{
    EpochIssuanceProof, EpochSessionApplication, EpochSessionError, EpochSessionResult, IssuedEpochSession, ResolvedEpochSession,
};
