//! Pure identity models and static ports, independent of application feature union.
//!
//! Credential operations and session runtimes stay in `rbit-identity`. Model consumers
//! depend here directly so enabling those operations elsewhere cannot widen this graph.

pub mod access_tokens;
pub mod account;
pub mod accounts;
pub mod credential;
pub mod locale;
pub mod names;
pub mod passkeys;
pub mod profiles;
pub mod recovery;
pub mod sessions;
pub mod token;
pub mod two_factor;

pub use account::{Account, AccountId, AccountKind, User};
pub use locale::AccountLocale;
pub use names::{AccountName, Email};
pub use token::TokenDigest;
