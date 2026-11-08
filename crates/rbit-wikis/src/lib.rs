//! Repository wiki contracts without storage, runtime, HTTP or rendering work.
//!
//! Validated names are not filesystem capabilities or safe HTML. Revision identities
//! do not prove object existence, authorization or a completed durable write. Policy
//! consumes trusted current repository facts; persistence must enforce its own atomic
//! CAS and permission/archive checks. Explicit adapters own storage/native work.

pub mod change;
pub mod commands;
mod content;
mod identity;
pub mod listing;
mod names;
pub mod ownership;
mod page;
pub mod policy;
pub mod reading;
mod revision;
pub mod startup;
mod store;

pub use content::{MAX_WIKI_CONTENT_BYTES, WikiContent, WikiContentError};
pub use identity::{WikiGeneration, WikiId, WikiPageId};
pub use names::{WikiNameError, WikiPath, WikiTitle};
pub use page::WikiPage;
pub use revision::{WikiObjectFormat, WikiRevisionError, WikiRevisionId};
pub use store::{WikiStoreId, WikiStoreIdError};
