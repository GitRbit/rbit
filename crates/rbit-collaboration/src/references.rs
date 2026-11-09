//! Reference parsing contracts and application-owned resolution/effects.
mod models;
mod plain;
mod pushes;
mod resolved;
pub use models::{CLOSING_KEYWORDS, Found, MAX_MENTIONS, Reference, RemoteRef, Scan};
pub use plain::{find, scan_plain};
pub use pushes::push_exclusions;
pub use resolved::{ReferenceSet, ReferencedIssue};
#[cfg(feature = "applications")]
mod markdown;
#[cfg(feature = "applications")]
pub use markdown::{markdown_options, scan_markdown};
#[cfg(feature = "applications")]
mod cross_references;
#[cfg(feature = "applications")]
mod ports;
#[cfg(feature = "applications")]
mod resolver;
#[cfg(feature = "applications")]
pub use cross_references::CrossReferences;
#[cfg(feature = "applications")]
pub use ports::{CrossReferencePort, ReferenceReadPort, RemoteReferencePort};
#[cfg(feature = "applications")]
pub use resolver::ReferenceResolver;
#[cfg(feature = "applications")]
mod commit_effects;
#[cfg(feature = "applications")]
pub use commit_effects::{CommitReference, CommitReferencePort, CommitReferences, MergeIssueClosures, MergeReferencePort};
#[cfg(feature = "applications")]
mod suggestions;
#[cfg(feature = "applications")]
pub use suggestions::{ReferenceSuggestions, SuggestionError, SuggestionPort, SuggestionThreadPort};
