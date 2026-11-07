//! Stable session DTOs, owner projections and static persistence contracts.

mod epoch;
mod identity;
mod promotion;
mod record;
mod recovery;
mod repository;
mod user;
mod view;

pub use epoch::{EpochSessionRepository, EpochSessionSnapshot, EpochSessionUserRepository};
pub use identity::SessionPairIdentity;
pub use promotion::{SessionPromotion, SessionRevocationDecision, SessionRevocationOutcome, SessionRevocationRequest};
pub use record::{SessionRecord, StoredSession};
pub use repository::{SessionRepository, SessionUserRepository};
pub use user::SessionUser;
pub use view::{ActiveSession, ClientInfo, session_id};

#[cfg(test)]
mod tests;

pub use recovery::{
    CompletedSessionRevocation, PendingSessionRevocation, SessionRevocationAcknowledgment, SessionRevocationRepository,
    SessionRevocationState,
};
