//! Bounded, data-only Fluent language packs and immutable in-memory providers.
//! This crate performs no disk, HTTP or database access and publishes no catalogs.

mod archive;
mod catalog;
mod locale;
mod provider;

pub use archive::{MAX_ARCHIVE_BYTES, MAX_ENTRIES, MAX_ENTRY_BYTES, MAX_EXPANDED_BYTES, PackManifest};
pub use provider::{LanguagePack, LocaleInfo, PackSet};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("invalid or unsupported language archive")]
    Archive,
    #[error("language package resource limit exceeded")]
    Limit,
    #[error("unsafe language package entry path")]
    Path,
    #[error("invalid language package manifest")]
    Manifest,
    #[error("language package checksum mismatch or unlisted entry")]
    Checksum,
    #[error("unsupported locale or inconsistent direction")]
    Locale,
    #[error("incompatible message schema or unknown namespace/message/argument")]
    Schema,
    #[error("invalid or unsupported Fluent catalog")]
    Catalog,
    #[error("conflicting language providers, missing parent or parent cycle")]
    Conflict,
}
