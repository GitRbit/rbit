//! Release records: named, tagged versions of a repository with notes and attached files.
//!
//! A release always names an existing tag of its repository (one release per tag). It
//! is either a **draft** (visible to writers only, not yet announced) or **published**
//! (`published_at` is set the first time it stops being a draft). Attached files
//! ([`ReleaseAsset`]) are stored outside the database; only their metadata lives here.
//! `rbit_core::release` re-exports this module at its original path.

use rbit_contracts::{AccountId, RepoId, Timestamp};
/// Longest release title, in characters.
pub const MAX_RELEASE_NAME_CHARS: usize = 255;
/// Longest release notes, in characters.
pub const MAX_RELEASE_NOTES_CHARS: usize = 125_000;
/// Longest attached file name, in bytes.
pub const MAX_ASSET_NAME_BYTES: usize = 255;

/// A release's database id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReleaseId(pub i64);

/// An attached file's database id (also its file name on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReleaseAssetId(pub i64);

/// Who created a release (absent once their account is deleted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAuthor {
    pub id: AccountId,
    pub name: String,
}

// Actions: releases from workflows.
/// Who made a release or uploaded one of its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReleaseSource {
    /// A person (or anything acting with their credentials).
    #[default]
    User,
    /// A workflow task, with its token. Such a release or file has no author.
    Actions,
}

impl ReleaseSource {
    /// The stored value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Actions => "actions",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::User, Self::Actions].into_iter().find(|source| source.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub id: ReleaseId,
    pub repo_id: RepoId,
    /// The tag's short name, e.g. `v1.2.0`.
    pub tag_name: String,
    /// The title; never empty (defaults to the tag name).
    pub name: String,
    /// Markdown.
    pub notes: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub author: Option<ReleaseAuthor>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// When it was first published; `None` for drafts that never were.
    pub published_at: Option<Timestamp>,
    // Actions: releases from workflows.
    pub source: ReleaseSource,
}

impl Release {
    /// The date shown for the release: its publication, or its creation for drafts.
    pub fn shown_at(&self) -> Timestamp {
        self.published_at.unwrap_or(self.created_at)
    }
}

/// Metadata of a file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub id: ReleaseAssetId,
    pub release_id: ReleaseId,
    pub repo_id: RepoId,
    pub name: String,
    pub size: u64,
    /// As declared by the uploader; informational only (downloads are always
    /// `application/octet-stream`).
    pub content_type: String,
    pub sha256: [u8; 32],
    pub download_count: u64,
    pub uploader_id: Option<AccountId>,
    pub created_at: Timestamp,
    // Actions: releases from workflows.
    pub source: ReleaseSource,
}

impl ReleaseAsset {
    /// The SHA-256 digest as lower-case hex.
    pub fn sha256_hex(&self) -> String {
        use std::fmt::Write as _;
        self.sha256.iter().fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
    }
}

/// Checks a release title (already trimmed); empty titles are replaced by the tag name
/// before this is called.
pub fn validate_release_name(name: &str) -> Result<(), String> {
    if name.chars().count() > MAX_RELEASE_NAME_CHARS {
        return Err(format!("the title must be at most {MAX_RELEASE_NAME_CHARS} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err("the title cannot contain control characters".to_owned());
    }
    Ok(())
}

pub fn validate_release_notes(notes: &str) -> Result<(), String> {
    if notes.chars().count() > MAX_RELEASE_NOTES_CHARS {
        return Err(format!("release notes must be at most {MAX_RELEASE_NOTES_CHARS} characters"));
    }
    Ok(())
}

/// Checks an attached file name: 1–255 bytes, no `/`, `\` or control characters, no
/// leading dot, no surrounding whitespace. Names never become paths (files are stored by
/// id), but they appear in URLs and `Content-Disposition` headers.
pub fn validate_asset_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("the file needs a name".to_owned());
    }
    if name.len() > MAX_ASSET_NAME_BYTES {
        return Err(format!("file names must be at most {MAX_ASSET_NAME_BYTES} bytes"));
    }
    if name.trim() != name {
        return Err("file names cannot start or end with whitespace".to_owned());
    }
    if name.starts_with('.') {
        return Err("file names cannot start with a dot".to_owned());
    }
    if name.chars().any(|c| c == '/' || c == '\\' || c.is_control()) {
        return Err("file names cannot contain slashes, backslashes or control characters".to_owned());
    }
    Ok(())
}

/// A release to insert.
#[derive(Debug, Clone)]
pub struct NewRelease<'a> {
    pub repo_id: RepoId,
    pub tag_name: &'a str,
    pub name: &'a str,
    pub notes: &'a str,
    pub is_draft: bool,
    pub is_prerelease: bool,
    /// `None` for a release a workflow task made.
    pub author_id: Option<AccountId>,
    pub source: ReleaseSource,
}

/// The editable fields of a release. `published_at` is only ever set once: pass the
/// publication time when a draft is published, `None` to keep the stored value.
#[derive(Debug, Clone)]
pub struct ReleaseChanges<'a> {
    pub tag_name: &'a str,
    pub name: &'a str,
    pub notes: &'a str,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: Option<Timestamp>,
}

/// Metadata of an uploaded file, inserted once its bytes are safely on disk.
#[derive(Debug, Clone)]
pub struct NewAsset<'a> {
    pub release_id: ReleaseId,
    pub repo_id: RepoId,
    pub name: &'a str,
    pub size: u64,
    pub content_type: &'a str,
    pub sha256: [u8; 32],
    /// `None` for a file a workflow task uploaded.
    pub uploader_id: Option<AccountId>,
    pub source: ReleaseSource,
}

/// Releases per page of the releases list.
pub const RELEASES_PER_PAGE: u32 = 10;
/// Raw release form input, exactly as submitted.
#[derive(Debug, Clone, Default)]
pub struct ReleaseInput {
    pub tag_name: String,
    pub name: String,
    pub notes: String,
    pub is_draft: bool,
    pub is_prerelease: bool,
    /// When creating a release for a tag that does not exist yet: the branch (or commit)
    /// to create it from. Empty means the tag must already exist.
    pub target: String,
}

/// A release with its attached files.
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub release: Release,
    pub assets: Vec<ReleaseAsset>,
}

/// One page of the releases list.
#[derive(Debug, Clone)]
pub struct ReleasePage {
    pub releases: Vec<ReleaseDetail>,
    pub total: u64,
    /// 1-based page actually served.
    pub page: u32,
}

impl ReleasePage {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(RELEASES_PER_PAGE)).max(1)).unwrap_or(u32::MAX)
    }
}

/// `512 MiB`, `10 KiB` — for limit messages.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1 << 30, "GiB"), (1 << 20, "MiB"), (1 << 10, "KiB")];
    for (unit, label) in UNITS {
        if bytes >= unit && bytes.is_multiple_of(unit) {
            return format!("{} {label}", bytes / unit);
        }
    }
    format!("{bytes} bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names() {
        for good in ["app.tar.gz", "rbit Setup 1.0.exe", "ü.zip", "a"] {
            assert!(validate_asset_name(good).is_ok(), "{good}");
        }
        let long = "a".repeat(256);
        for bad in ["", ".hidden", "../x", "a/b", "a\\b", "tab\tname", " lead", "trail ", "nul\0", long.as_str()] {
            assert!(validate_asset_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn titles_and_notes() {
        assert!(validate_release_name("Version 1.0").is_ok());
        assert!(validate_release_name(&"x".repeat(256)).is_err());
        assert!(validate_release_name("line\nbreak").is_err());
        assert!(validate_release_notes(&"x".repeat(MAX_RELEASE_NOTES_CHARS)).is_ok());
        assert!(validate_release_notes(&"x".repeat(MAX_RELEASE_NOTES_CHARS + 1)).is_err());
    }

    #[test]
    fn digests_render_as_hex() {
        let mut sha256 = [0u8; 32];
        sha256[0] = 0xab;
        sha256[31] = 0x01;
        let asset = ReleaseAsset {
            id: ReleaseAssetId(1),
            release_id: ReleaseId(1),
            repo_id: RepoId(1),
            name: "a".into(),
            size: 0,
            content_type: String::new(),
            sha256,
            download_count: 0,
            uploader_id: None,
            created_at: Timestamp(0),
            source: ReleaseSource::User,
        };
        let hex = asset.sha256_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.starts_with("ab00") && hex.ends_with("01"));
    }
}
