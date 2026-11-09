//! Account profile records and static persistence ports for self-service changes.

mod repository;

pub use repository::{AccountRenameRepository, ProfileRepository};

/// Editable profile fields of an account. `None` means "not set".
///
/// `display_name` and `bio` are shared with organisations; `website` and `location` are
/// user profile extras. Storage decides where each lives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountProfile {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub website: Option<String>,
    pub location: Option<String>,
}

/// The profile fields shown on an account page in addition to its display name and bio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicProfile {
    /// Always an `http://` or `https://` URL.
    pub website: Option<String>,
    pub location: Option<String>,
}
