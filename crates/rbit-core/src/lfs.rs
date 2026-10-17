//! Git Large File Storage: object ids, pointer files and file locks.
//!
//! Milestone 3 workstream L (Git LFS) owns this file.
//!
//! With LFS, a repository commits a small *pointer file* in place of a large file; the
//! content itself is an *object* named by its SHA-256 digest ([`LfsOid`]) and stored by
//! the server outside Git. Objects are content-addressed, so one stored object can belong
//! to many repositories. *Locks* ([`LfsLock`]) mark a path as being edited by one user,
//! so others do not change files that cannot be merged.

use std::fmt;

use crate::{AccountId, RepoId, Timestamp};

/// Longest pointer file that is recognised as one (the specification's limit).
pub const MAX_POINTER_BYTES: usize = 1024;
/// Longest path that may be locked, in bytes.
pub const MAX_LOCK_PATH_BYTES: usize = 1024;

/// The first line of every pointer file (and of the pre-release spelling).
const POINTER_VERSIONS: [&str; 2] = ["version https://git-lfs.github.com/spec/v1", "version https://hawser.github.com/spec/v1"];

/// An object id: the SHA-256 digest of the content, as 64 lower-case hex digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LfsOid(String);

/// Why a value is not an object id.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an object id is a SHA-256 digest: 64 lower-case hexadecimal digits")]
pub struct InvalidOid;

impl LfsOid {
    /// Accepts exactly 64 lower-case hex digits (what clients send; never a path).
    pub fn parse(value: &str) -> Result<Self, InvalidOid> {
        let valid = value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid { Ok(Self(value.to_owned())) } else { Err(InvalidOid) }
    }

    /// The id of content with this SHA-256 digest.
    pub fn from_digest(digest: [u8; 32]) -> Self {
        use std::fmt::Write as _;
        Self(digest.iter().fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The two directory levels an object is stored under: `aa` and `bb` of `aabb…`.
    pub fn shards(&self) -> (&str, &str) {
        (&self.0[..2], &self.0[2..4])
    }
}

impl fmt::Display for LfsOid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a client wants to do with objects (the batch API's `operation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfsOperation {
    Upload,
    Download,
}

impl LfsOperation {
    /// `upload` or `download`; anything else is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "upload" => Some(Self::Upload),
            "download" => Some(Self::Download),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Download => "download",
        }
    }

    /// Uploads need write access, downloads read access.
    pub fn is_write(self) -> bool {
        self == Self::Upload
    }
}

/// A pointer file: what Git stores in place of an LFS-tracked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsPointer {
    pub oid: LfsOid,
    pub size: u64,
}

impl LfsPointer {
    /// Recognises a pointer file: at most [`MAX_POINTER_BYTES`], the version line first,
    /// then `key value` lines that include `oid sha256:<id>` and `size <bytes>`
    /// (extension lines are allowed and ignored). Anything else is an ordinary file.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() > MAX_POINTER_BYTES {
            return None;
        }
        let text = std::str::from_utf8(data).ok()?;
        let mut lines = text.strip_suffix('\n').unwrap_or(text).split('\n');
        if !POINTER_VERSIONS.contains(&lines.next()?) {
            return None;
        }
        let (mut oid, mut size) = (None, None);
        for line in lines {
            let (key, value) = line.split_once(' ')?;
            match key {
                "oid" => oid = Some(LfsOid::parse(value.strip_prefix("sha256:")?).ok()?),
                "size" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => size = Some(value.parse().ok()?),
                "size" => return None,
                _ if !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.')) => {}
                _ => return None,
            }
        }
        Some(Self { oid: oid?, size: size? })
    }

    /// The canonical pointer file text.
    pub fn to_text(&self) -> String {
        format!("{}\noid sha256:{}\nsize {}\n", POINTER_VERSIONS[0], self.oid, self.size)
    }
}

/// A lock's database id (the locks API's `id`, as a string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LfsLockId(pub i64);

/// Who holds a lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockOwner {
    pub id: AccountId,
    pub name: String,
}

/// A path of a repository locked by one user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsLock {
    pub id: LfsLockId,
    pub repo_id: RepoId,
    /// As the client sent it (relative to the repository root).
    pub path: String,
    pub owner: LockOwner,
    pub locked_at: Timestamp,
}

/// Checks a path to lock: 1–[`MAX_LOCK_PATH_BYTES`] bytes without control characters.
/// Locks are advisory and paths are only compared, never opened.
pub fn validate_lock_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("the path to lock is missing".to_owned());
    }
    if path.len() > MAX_LOCK_PATH_BYTES {
        return Err(format!("paths can be at most {MAX_LOCK_PATH_BYTES} bytes"));
    }
    if path.chars().any(char::is_control) {
        return Err("paths cannot contain control characters".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393";

    #[test]
    fn object_ids_are_exactly_64_lower_case_hex_digits() {
        let oid = LfsOid::parse(OID).unwrap();
        assert_eq!(oid.shards(), ("4d", "7a"));
        assert_eq!(oid.to_string(), OID);
        for bad in ["", "4d7a", &OID.to_uppercase(), &format!("{OID}0"), &OID.replace('4', "g"), "../../../../../../../../etc/passwd"] {
            assert_eq!(LfsOid::parse(bad), Err(InvalidOid), "{bad:?}");
        }
        assert_eq!(LfsOid::from_digest([0xab; 32]).as_str(), "ab".repeat(32));
    }

    #[test]
    fn operations() {
        assert_eq!(LfsOperation::parse("upload"), Some(LfsOperation::Upload));
        assert_eq!(LfsOperation::parse("download").map(LfsOperation::as_str), Some("download"));
        assert_eq!(LfsOperation::parse("Upload"), None);
        assert!(LfsOperation::Upload.is_write() && !LfsOperation::Download.is_write());
    }

    #[test]
    fn pointer_files_are_recognised() {
        let text = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{OID}\nsize 12345\n");
        let pointer = LfsPointer::parse(text.as_bytes()).unwrap();
        assert_eq!(pointer, LfsPointer { oid: LfsOid::parse(OID).unwrap(), size: 12345 });
        assert_eq!(pointer.to_text(), text);

        let with_extension = format!("version https://git-lfs.github.com/spec/v1\next-0-foo sha256:{OID}\noid sha256:{OID}\nsize 0\n");
        assert_eq!(LfsPointer::parse(with_extension.as_bytes()).map(|p| p.size), Some(0));
        let legacy = format!("version https://hawser.github.com/spec/v1\noid sha256:{OID}\nsize 1");
        assert!(LfsPointer::parse(legacy.as_bytes()).is_some(), "the final newline is optional");
    }

    #[test]
    fn ordinary_files_are_not_pointers() {
        let good = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{OID}\nsize 12\n");
        for bad in [
            String::new(),
            "hello\n".to_owned(),
            good.replace("size 12", "size -12"),
            good.replace("size 12", "size 1e3"),
            good.replace("size 12\n", ""),
            good.replace(&format!("oid sha256:{OID}\n"), ""),
            good.replace("sha256:", "md5:"),
            good.replace("spec/v1", "spec/v2"),
            good.replace('\n', "\r\n"),
            format!("{good}\n"),
            format!("{good}{}\n", "x".repeat(MAX_POINTER_BYTES)),
        ] {
            assert_eq!(LfsPointer::parse(bad.as_bytes()), None, "{bad:?}");
        }
        assert_eq!(LfsPointer::parse(&[0xff, 0xfe]), None);
    }

    #[test]
    fn lock_paths() {
        assert!(validate_lock_path("assets/logo.psd").is_ok());
        assert!(validate_lock_path("ü/ä.bin").is_ok());
        for bad in [String::new(), "a\0b".to_owned(), "tab\tname".to_owned(), "a".repeat(MAX_LOCK_PATH_BYTES + 1)] {
            assert!(validate_lock_path(&bad).is_err(), "{bad:?}");
        }
    }
}
