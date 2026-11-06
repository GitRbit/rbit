//! Stable session DTOs, owner projections and static persistence contracts.

mod epoch;
mod record;
mod repository;
mod user;
mod view;

pub use epoch::{EpochSessionRepository, EpochSessionSnapshot, EpochSessionUserRepository};
pub use record::{SessionRecord, StoredSession};
pub use repository::{SessionRepository, SessionUserRepository};
pub use user::SessionUser;
pub use view::{ActiveSession, ClientInfo, session_id};

#[cfg(test)]
mod tests;
