//! Session data contracts without runtime, crypto, or persistence dependencies.

mod record;
mod repository;
mod user;
mod view;

pub use record::{SessionRecord, StoredSession};
pub use repository::{SessionRepository, SessionUserRepository};
pub use user::SessionUser;
pub use view::{ActiveSession, ClientInfo, session_id};

#[cfg(test)]
mod tests;

#[cfg(feature = "sessions")]
mod application;
#[cfg(feature = "sessions")]
pub use application::{SessionApplication, SessionClock, SessionError, SessionPolicy, SessionResult, SystemSessionClock};

#[cfg(feature = "sessions")]
pub mod effects;
