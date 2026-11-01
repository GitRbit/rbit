//! Streaming validation of a deliberately narrow, uncompressed USTAR profile.
//!
//! Validation establishes agreement between a package manifest and its payload
//! bytes. It does not install files, grant trust/capabilities, validate executable
//! formats, run code, or establish a sandbox. A successful summary owns metadata,
//! never payloads. Hosts must separately enforce input transport deadlines.
//!
//! See the crate README for the exact accepted header and termination profile.
//! General tar extensions and extra record padding are intentionally rejected.
//! `PackageStore` separately stages validated bytes in a private host-owned root;
//! its publication receipts do not grant installation or execution authority.

mod store;

pub use store::{
    CleanupStatus, Durability, PackageReceipt, PackageStore, PreparedPackage, PublicationKind, PublicationVisibility, StageControl,
    StoreError, StoreFailure,
};

use std::{
    collections::BTreeMap,
    io::{self, Read},
};

use rbit_plugin_api::{MAX_PACKAGE_BYTES, MAX_PACKAGE_FILES, MAX_PACKAGE_MANIFEST_BYTES, PackageManifest, PackagePath, Sha256Digest};
use sha2::{Digest, Sha256};

const BLOCK_BYTES: usize = 512;
const BUFFER_BYTES: usize = 64 * 1024;
const MANIFEST_PATH: &str = "package.json";

/// Includes every header, the largest possible padding, both terminal blocks,
/// and one byte for the final EOF/trailing-data probe. No read exceeds this cap.
pub const MAX_ARCHIVE_WIRE_BYTES: u64 = MAX_PACKAGE_BYTES
    + MAX_PACKAGE_MANIFEST_BYTES as u64
    + (MAX_PACKAGE_FILES as u64 + 1) * (BLOCK_BYTES as u64 + BLOCK_BYTES as u64 - 1)
    + 2 * BLOCK_BYTES as u64
    + 1;

/// A verified descriptor and byte accounting, without payload or install authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArchive {
    manifest: PackageManifest,
    wire_bytes: u64,
    payload_bytes: u64,
}
impl ValidatedArchive {
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }
    pub const fn wire_bytes(&self) -> u64 {
        self.wire_bytes
    }
    pub const fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }
}

/// Bounded diagnostics: archive contents and JSON field names never enter Display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ArchiveError {
    #[error("package archive input read failed")]
    Read,
    #[error("package archive input is truncated")]
    Truncated,
    #[error("package archive exceeds its wire byte limit")]
    WireLimit,
    #[error("package archive header violates the supported USTAR profile")]
    Header,
    #[error("package archive header checksum is invalid")]
    Checksum,
    #[error("package archive path is invalid")]
    Path,
    #[error("package archive must begin with the regular package.json descriptor")]
    ManifestFirst,
    #[error("package manifest byte length is outside its bounds")]
    ManifestSize,
    #[error("package manifest is invalid")]
    Manifest,
    #[error("package manifest declares the reserved descriptor path")]
    ReservedPath,
    #[error("package archive contains an undeclared or duplicate file")]
    UnexpectedFile,
    #[error("package archive file size differs from its declaration")]
    FileSize,
    #[error("package archive file digest differs from its declaration")]
    Digest,
    #[error("package archive is missing a declared payload file")]
    MissingFile,
    #[error("package archive contains nonzero body padding")]
    Padding,
    #[error("package archive must end with exactly two zero blocks")]
    Terminator,
    #[error("package archive contains trailing data")]
    TrailingData,
}

/// Validates one complete package using fixed-buffer payload hashing.
///
/// The first entry is the descriptor, bounded before allocation or JSON parsing.
/// Subsequent regular files may appear in any order, exactly once each. Success
/// requires actual EOF immediately after two zero blocks. The reader must describe
/// a complete archive, rather than a shared stream containing a following message.
/// Blocking readers require a host-enforced deadline; byte bounds do not impose one.
pub fn validate_archive(reader: impl Read) -> Result<ValidatedArchive, ArchiveError> {
    let mut input = WireReader { reader, bytes: 0 };
    let mut block = [0; BLOCK_BYTES];
    input.read_exact(&mut block)?;
    if block.iter().all(|byte| *byte == 0) {
        return Err(ArchiveError::ManifestFirst);
    }
    let header = parse_header(&block)?;
    if header.path.as_str() != MANIFEST_PATH {
        return Err(ArchiveError::ManifestFirst);
    }
    if header.size == 0 || header.size > MAX_PACKAGE_MANIFEST_BYTES as u64 {
        return Err(ArchiveError::ManifestSize);
    }
    let mut descriptor = vec![0; usize::try_from(header.size).map_err(|_| ArchiveError::ManifestSize)?];
    input.read_exact(&mut descriptor)?;
    read_padding(&mut input, header.size)?;
    let manifest: PackageManifest = serde_json::from_slice(&descriptor).map_err(|_| ArchiveError::Manifest)?;
    // PackageManifest's custom Deserialize validates declarations before use.
    let mut expected = BTreeMap::new();
    for file in &manifest.files {
        let folded = file.path.as_str().to_ascii_lowercase();
        if folded == MANIFEST_PATH || folded.starts_with("package.json/") {
            return Err(ArchiveError::ReservedPath);
        }
        expected.insert(file.path.as_str(), file);
    }
    let mut payload_bytes = 0_u64;
    let mut buffer = vec![0; BUFFER_BYTES].into_boxed_slice();
    for _ in 0..manifest.files.len() {
        input.read_exact(&mut block)?;
        if block.iter().all(|byte| *byte == 0) {
            return Err(ArchiveError::MissingFile);
        }
        let header = parse_header(&block)?;
        let file = expected.remove(header.path.as_str()).ok_or(ArchiveError::UnexpectedFile)?;
        if header.size != file.size {
            return Err(ArchiveError::FileSize);
        }
        payload_bytes = payload_bytes.checked_add(header.size).filter(|size| *size <= MAX_PACKAGE_BYTES).ok_or(ArchiveError::WireLimit)?;
        let mut digest = Sha256::new();
        let mut remaining = header.size;
        while remaining != 0 {
            let count = usize::try_from(remaining.min(BUFFER_BYTES as u64)).map_err(|_| ArchiveError::FileSize)?;
            input.read_exact(&mut buffer[..count])?;
            digest.update(&buffer[..count]);
            remaining -= count as u64;
        }
        if !digest_matches(&digest.finalize(), &file.sha256) {
            return Err(ArchiveError::Digest);
        }
        read_padding(&mut input, header.size)?;
    }
    if !expected.is_empty() {
        return Err(ArchiveError::MissingFile);
    }
    // Read each block separately: no optional record padding, concatenated archives,
    // bare EOF or single-zero termination can be accepted as a complete package.
    for _ in 0..2 {
        input.read_exact(&mut block)?;
        if block.iter().any(|byte| *byte != 0) {
            return Err(ArchiveError::Terminator);
        }
    }
    if input.read(&mut [0; 1])? != 0 {
        return Err(ArchiveError::TrailingData);
    }
    Ok(ValidatedArchive { manifest, wire_bytes: input.bytes, payload_bytes })
}

struct WireReader<R> {
    reader: R,
    bytes: u64,
}
impl<R: Read> WireReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, ArchiveError> {
        let available = MAX_ARCHIVE_WIRE_BYTES.checked_sub(self.bytes).ok_or(ArchiveError::WireLimit)?;
        let count = usize::try_from(available.min(buffer.len() as u64)).map_err(|_| ArchiveError::WireLimit)?;
        if count == 0 {
            return Err(ArchiveError::WireLimit);
        }
        let read = loop {
            match self.reader.read(&mut buffer[..count]) {
                Ok(read) => break read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(ArchiveError::Read),
            }
        };
        self.bytes = self.bytes.checked_add(read as u64).ok_or(ArchiveError::WireLimit)?;
        Ok(read)
    }
    fn read_exact(&mut self, mut buffer: &mut [u8]) -> Result<(), ArchiveError> {
        while !buffer.is_empty() {
            let read = self.read(buffer)?;
            if read == 0 {
                return Err(ArchiveError::Truncated);
            }
            buffer = &mut buffer[read..];
        }
        Ok(())
    }
}

struct Header {
    path: PackagePath,
    size: u64,
}
fn parse_header(block: &[u8; BLOCK_BYTES]) -> Result<Header, ArchiveError> {
    if &block[257..263] != b"ustar\0"
        || &block[263..265] != b"00"
        || block[156] != b'0'
        || block[157..257].iter().any(|byte| *byte != 0)
        || block[265..329].iter().any(|byte| *byte != 0)
        || block[500..].iter().any(|byte| *byte != 0)
    {
        return Err(ArchiveError::Header);
    }
    let checksum = octal(&block[148..156])?;
    let actual: u64 = block[..148].iter().chain(&block[156..]).map(|byte| u64::from(*byte)).sum::<u64>() + 8 * u64::from(b' ');
    if checksum != actual {
        return Err(ArchiveError::Checksum);
    }
    if !matches!(octal(&block[100..108])?, 0o644 | 0o755)
        || octal(&block[108..116])? != 0
        || octal(&block[116..124])? != 0
        || octal(&block[136..148])? != 0
    {
        return Err(ArchiveError::Header);
    }
    for device in [&block[329..337], &block[337..345]] {
        if device.iter().any(|byte| *byte != 0) && octal(device)? != 0 {
            return Err(ArchiveError::Header);
        }
    }
    let size = octal(&block[124..136])?;
    let name = field(&block[..100])?;
    let prefix = field(&block[345..500])?;
    if name.is_empty() {
        return Err(ArchiveError::Path);
    }
    let path = if prefix.is_empty() { name.to_owned() } else { format!("{prefix}/{name}") };
    let path = path.parse().map_err(|_| ArchiveError::Path)?;
    Ok(Header { path, size })
}
fn field(bytes: &[u8]) -> Result<&str, ArchiveError> {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    if bytes[end..].iter().any(|byte| *byte != 0) {
        return Err(ArchiveError::Path);
    }
    std::str::from_utf8(&bytes[..end]).map_err(|_| ArchiveError::Path)
}
fn octal(bytes: &[u8]) -> Result<u64, ArchiveError> {
    let (terminator, digits) = bytes.split_last().ok_or(ArchiveError::Header)?;
    if *terminator != 0 || digits.is_empty() || !digits.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
        return Err(ArchiveError::Header);
    }
    digits.iter().try_fold(0_u64, |value, byte| {
        value.checked_mul(8).and_then(|value| value.checked_add(u64::from(*byte - b'0'))).ok_or(ArchiveError::Header)
    })
}
fn read_padding(input: &mut WireReader<impl Read>, size: u64) -> Result<(), ArchiveError> {
    let padding = (BLOCK_BYTES as u64 - size % BLOCK_BYTES as u64) % BLOCK_BYTES as u64;
    let mut buffer = [0; BLOCK_BYTES];
    let count = usize::try_from(padding).map_err(|_| ArchiveError::Padding)?;
    input.read_exact(&mut buffer[..count])?;
    if buffer[..count].iter().any(|byte| *byte != 0) {
        return Err(ArchiveError::Padding);
    }
    Ok(())
}
fn digest_matches(actual: &[u8], expected: &Sha256Digest) -> bool {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    if actual.len() != 32 {
        return false;
    }
    actual
        .iter()
        .zip(expected.as_str().as_bytes().as_chunks::<2>().0)
        .all(|(byte, pair)| pair[0] == HEX[usize::from(byte >> 4)] && pair[1] == HEX[usize::from(byte & 15)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn wire_budget_caps_each_read_including_the_trailing_probe() {
        let mut input = WireReader { reader: Cursor::new([1, 2]), bytes: MAX_ARCHIVE_WIRE_BYTES - 1 };
        let mut buffer = [0; 512];
        assert_eq!(input.read(&mut buffer), Ok(1));
        assert_eq!(input.bytes, MAX_ARCHIVE_WIRE_BYTES);
        assert_eq!(input.reader.position(), 1);
        assert_eq!(input.read(&mut [0; 1]), Err(ArchiveError::WireLimit));
        assert_eq!(input.reader.position(), 1, "budget exhaustion must not touch the underlying reader");
    }
}
