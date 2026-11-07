//! Repository wiki contracts without storage, runtime, HTTP or rendering work.
//!
//! Validated names are not filesystem capabilities or safe HTML. Revision identities
//! do not prove object existence, authorization or a completed durable write. Policy
//! consumes trusted current repository facts; persistence must enforce its own atomic
//! CAS and permission/archive checks. No production wiki backend exists in this slice.

mod content;
mod identity;
mod names;
mod page;
pub mod policy;
pub mod reading;
mod revision;
mod store;

pub use content::{MAX_WIKI_CONTENT_BYTES, WikiContent, WikiContentError};
pub use identity::{WikiGeneration, WikiPageId};
pub use names::{WikiNameError, WikiPath, WikiTitle};
pub use page::WikiPage;
pub use revision::{WikiObjectFormat, WikiRevisionError, WikiRevisionId};
pub use store::{WikiStoreId, WikiStoreIdError};
