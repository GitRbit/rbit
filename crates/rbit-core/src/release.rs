//! Releases: named, tagged versions of a repository with notes and attached files.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! A release always names an existing tag of its repository (one release per tag). It
//! is either a **draft** (visible to writers only, not yet announced) or **published**
//! (`published_at` is set the first time it stops being a draft). Attached files
//! ([`ReleaseAsset`]) are stored outside the database; only their metadata lives here.

use crate::{AccountId, RepoId, Timestamp};

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
        };
        let hex = asset.sha256_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.starts_with("ab00") && hex.ends_with("01"));
    }
}
