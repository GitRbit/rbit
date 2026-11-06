//! Session data contracts without runtime, crypto, or persistence dependencies.

mod record;
mod user;
mod view;

pub use record::{SessionRecord, StoredSession};
pub use user::SessionUser;
pub use view::{ActiveSession, ClientInfo, session_id};

#[cfg(test)]
mod tests;
