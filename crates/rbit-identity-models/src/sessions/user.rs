//! Feature-owned session identity projection, supplied by one repository lookup.

use crate::{AccountLocale, User};

/// An active (not suspended) user as a signed-in request sees them, from
/// the active-session account repository port.
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub user: User,
    /// An administrator provided the password; everything but replacing it is refused.
    pub password_change_required: bool,
    /// Something in the inbox is unread (always `false` when notifications are off).
    pub has_unread_notifications: bool,
    /// `None` follows browser/default negotiation; diagnostic locales are never stored.
    pub locale: Option<AccountLocale>,
}
