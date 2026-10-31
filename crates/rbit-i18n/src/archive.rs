//! The deliberately small, data-only `.rbit-language` ZIP32 profile.

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

use crate::{PackError, locale::validate_locale};

pub const MAX_ARCHIVE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 512 * 1024;
pub const MAX_ENTRIES: usize = 128;
const MAX_MANIFEST_BYTES: usize = 16 * 1024;

/// Publisher identity is descriptive metadata; checksums are integrity, not trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub schema: u32,
    pub id: String,
    pub version: Version,
    pub locale: String,
    pub direction: String,
    pub parent: Option<String>,
    pub message_schema: VersionReq,
    pub publisher: String,
    pub license: String,
    pub license_file: String,
    /// Namespace to catalog path. Only host-approved namespaces can be installed.
    pub catalogs: BTreeMap<String, String>,
    /// SHA-256 of every entry except `pack.toml`, including the license.
    pub checksums: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct Archive {
    pub manifest: PackManifest,
    pub files: BTreeMap<String, Vec<u8>>,
    pub digest: String,
}

impl Archive {
    pub fn parse(bytes: &[u8]) -> Result<Self, PackError> {
        let directory_start = preflight(bytes)?;
        let mut zip =
            zip::ZipArchive::with_config(zip::read::Config { archive_offset: zip::read::ArchiveOffset::Known(0) }, Cursor::new(bytes))
                .map_err(|_| PackError::Archive)?;
        if zip.has_duplicate_names() || zip.len() > MAX_ENTRIES || zip.is_empty() {
            return Err(PackError::Archive);
        }
        validate_layout(bytes, &zip, directory_start)?;
        zip.validate_header_consistency().map_err(|_| PackError::Archive)?;
        let mut total = 0u64;
        let mut files = BTreeMap::new();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|_| PackError::Archive)?;
            let path = std::str::from_utf8(entry.name_raw()).map_err(|_| PackError::Path)?.to_owned();
            if !safe_path(&path) {
                return Err(PackError::Path);
            }
            let mode = entry.unix_mode().unwrap_or(0o100_644);
            if !entry.is_file()
                || mode & 0o170_000 != 0o100_000
                || mode & 0o111 != 0
                || entry.encrypted()
                || !matches!(entry.compression(), zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated)
                || entry.extra_data().is_some_and(|extra| !extra.is_empty())
            {
                return Err(PackError::Archive);
            }
            total = total.checked_add(entry.size()).ok_or(PackError::Limit)?;
            if entry.size() > MAX_ENTRY_BYTES
                || total > MAX_EXPANDED_BYTES
                || entry.size() > entry.compressed_size().saturating_mul(200).saturating_add(1024)
            {
                return Err(PackError::Limit);
            }
            // Enforce the actual decoded bound as well as the untrusted directory sizes.
            let mut contents = Vec::new();
            entry.by_ref().take(MAX_ENTRY_BYTES + 1).read_to_end(&mut contents).map_err(|_| PackError::Archive)?;
            if contents.len() as u64 > MAX_ENTRY_BYTES || contents.len() as u64 != entry.size() {
                return Err(PackError::Limit);
            }
            if files.insert(path, contents).is_some() {
                return Err(PackError::Archive);
            }
        }
        let manifest_bytes = files.remove("pack.toml").ok_or(PackError::Manifest)?;
        if manifest_bytes.len() > MAX_MANIFEST_BYTES {
            return Err(PackError::Limit);
        }
        let manifest: PackManifest =
            toml::from_str(std::str::from_utf8(&manifest_bytes).map_err(|_| PackError::Manifest)?).map_err(|_| PackError::Manifest)?;
        manifest.validate()?;
        if files.len() != manifest.checksums.len() || !manifest.checksums.keys().eq(files.keys()) {
            return Err(PackError::Checksum);
        }
        for (path, contents) in &files {
            if manifest.checksums[path] != hex_digest(contents) {
                return Err(PackError::Checksum);
            }
        }
        // No unlisted data, scripts, assets or auxiliary archive links.
        let mut allowed: BTreeSet<_> = manifest.catalogs.values().cloned().collect();
        if allowed.len() != manifest.catalogs.len() || !allowed.insert(manifest.license_file.clone()) || !allowed.iter().eq(files.keys()) {
            return Err(PackError::Manifest);
        }
        let license = &files[&manifest.license_file];
        if license.is_empty() || license.len() > 64 * 1024 || std::str::from_utf8(license).is_err() {
            return Err(PackError::Manifest);
        }
        Ok(Self { manifest, files, digest: hex_digest(bytes) })
    }
}

impl PackManifest {
    fn validate(&self) -> Result<(), PackError> {
        if self.schema != 1
            || !identifier(&self.id)
            || self.catalogs.is_empty()
            || self.catalogs.len() > 32
            || self.publisher.is_empty()
            || self.publisher.len() > 256
            || self.publisher.chars().any(char::is_control)
            || self.license.is_empty()
            || self.license.len() > 128
            || self.license.chars().any(char::is_control)
            || !safe_path(&self.license_file)
            || ftl_extension(&self.license_file)
            || self.version.to_string().len() > 128
            || self.message_schema.to_string().len() > 128
        {
            return Err(PackError::Manifest);
        }
        validate_locale(&self.locale, Some(&self.direction))?;
        if let Some(parent) = &self.parent {
            validate_locale(parent, None)?;
            if parent == &self.locale || parent == "en" {
                return Err(PackError::Locale);
            }
        }
        for (namespace, path) in &self.catalogs {
            if !identifier(namespace) || !safe_path(path) || !path.starts_with("catalogs/") || !ftl_extension(path) {
                return Err(PackError::Manifest);
            }
        }
        for (path, hash) in &self.checksums {
            if !safe_path(path)
                || path == "pack.toml"
                || hash.len() != 64
                || !hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(PackError::Manifest);
            }
        }
        Ok(())
    }
}

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
        && value.as_bytes()[0].is_ascii_lowercase()
}

fn safe_path(path: &str) -> bool {
    path.len() <= 200
        && !path.is_empty()
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
}

pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [char::from(HEX[usize::from(byte >> 4)]), char::from(HEX[usize::from(byte & 15)])])
        .collect()
}

/// Limit allocation before ZIP constructs its central-directory index. No ZIP64,
/// comments, concatenated archives or multi-disk packages are part of schema 1.
fn preflight(bytes: &[u8]) -> Result<u64, PackError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(PackError::Limit);
    }
    if bytes.len() < 22
        || !bytes.starts_with(b"PK\x03\x04")
        || (bytes.len() >= 42 && &bytes[bytes.len() - 42..bytes.len() - 38] == b"PK\x06\x07")
    {
        return Err(PackError::Archive);
    }
    let end = &bytes[bytes.len() - 22..];
    let word = |offset| u16::from_le_bytes([end[offset], end[offset + 1]]);
    let dword = |offset| u32::from_le_bytes([end[offset], end[offset + 1], end[offset + 2], end[offset + 3]]);
    if &end[..4] != b"PK\x05\x06"
        || word(4) != 0
        || word(6) != 0
        || word(8) != word(10)
        || word(20) != 0
        || usize::from(word(10)) > MAX_ENTRIES
        || word(10) == 0
        || u64::from(dword(12)) + u64::from(dword(16)) + 22 != bytes.len() as u64
    {
        return Err(PackError::Archive);
    }
    Ok(u64::from(dword(16)))
}

fn ftl_extension(path: &str) -> bool {
    path.rsplit('.').next() == Some("ftl")
}

/// ZIP's decoder verifies headers and overlap; this profile additionally rejects
/// orphan local entries/gaps and descriptor/extra/comment metadata not in its
/// manifest. Exactly the listed file ranges and central records cover the ZIP.
fn validate_layout(bytes: &[u8], zip: &zip::ZipArchive<Cursor<&[u8]>>, directory_start: u64) -> Result<(), PackError> {
    if zip.central_directory_start() != directory_start {
        return Err(PackError::Archive);
    }
    let mut ranges = Vec::new();
    let mut central_end = directory_start;
    for index in 0..zip.len() {
        let entry = zip.by_index_data(index).map_err(|_| PackError::Archive)?;
        let start = usize::try_from(entry.header_start()).map_err(|_| PackError::Archive)?;
        let local = bytes.get(start..start.checked_add(30).ok_or(PackError::Archive)?).ok_or(PackError::Archive)?;
        if local[28..30] != [0, 0] || local[6] & 8 != 0 {
            return Err(PackError::Archive);
        }
        let name_len = u64::from(u16::from_le_bytes([local[26], local[27]]));
        let end = entry
            .header_start()
            .checked_add(30 + name_len)
            .and_then(|start| start.checked_add(entry.compressed_size()))
            .ok_or(PackError::Archive)?;
        ranges.push((entry.header_start(), end));
        if entry.central_header_start() != central_end {
            return Err(PackError::Archive);
        }
        let central_start = usize::try_from(central_end).map_err(|_| PackError::Archive)?;
        let central = bytes.get(central_start..central_start.checked_add(46).ok_or(PackError::Archive)?).ok_or(PackError::Archive)?;
        if central[30..34] != [0, 0, 0, 0] {
            return Err(PackError::Archive);
        }
        central_end += 46 + u64::from(u16::from_le_bytes([central[28], central[29]]));
    }
    ranges.sort_unstable();
    if ranges.first().is_none_or(|range| range.0 != 0)
        || ranges.last().is_none_or(|range| range.1 != directory_start)
        || ranges.windows(2).any(|pair| pair[0].1 != pair[1].0)
        || central_end != bytes.len() as u64 - 22
    {
        return Err(PackError::Archive);
    }
    Ok(())
}
