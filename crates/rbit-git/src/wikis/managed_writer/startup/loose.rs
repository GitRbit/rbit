//! Decode one bounded loose stream exactly, including its checksum and EOF.
use super::WikiObjectError;
use rbit_wikis::startup::objects::{MAX_SCAN_OBJECT_BYTES, WikiScannedKind};
use std::{fs::File, io::Read, path::Path};

pub(super) struct Decoded {
    pub(super) data: Vec<u8>,
    pub(super) offset: usize,
    pub(super) kind: WikiScannedKind,
}
pub(super) fn read(path: &Path, expected: gix::ObjectId) -> Result<Decoded, WikiObjectError> {
    let io = |error| WikiObjectError::from(crate::GitError::Io(error));
    let file = File::open(path).map_err(io)?;
    let maximum = u64::from(MAX_SCAN_OBJECT_BYTES) * 2;
    if !file.metadata().map_err(io)?.is_file() || file.metadata().map_err(io)?.len() > maximum {
        return Err(WikiObjectError::Format);
    }
    let mut compressed = Vec::new();
    file.take(maximum + 1).read_to_end(&mut compressed).map_err(io)?;
    if u64::try_from(compressed.len()).map_err(|_| WikiObjectError::Format)? > maximum {
        return Err(WikiObjectError::Format);
    }
    let mut data = vec![0; MAX_SCAN_OBJECT_BYTES as usize + 64];
    let mut decoder = gix::zlib::Decompress::default();
    let status = decoder.decompress(&compressed, &mut data, gix::zlib::FlushDecompress::Finish).map_err(crate::GitError::backend)?;
    if status != gix::zlib::Status::StreamEnd
        || decoder.total_in() != u64::try_from(compressed.len()).map_err(|_| WikiObjectError::Format)?
    {
        return Err(WikiObjectError::Format);
    }
    data.truncate(usize::try_from(decoder.total_out()).map_err(|_| WikiObjectError::Format)?);
    let end = data.iter().take(64).position(|byte| *byte == 0).ok_or(WikiObjectError::Format)?;
    let (kind, size) =
        std::str::from_utf8(&data[..end]).map_err(|_| WikiObjectError::Format)?.split_once(' ').ok_or(WikiObjectError::Format)?;
    if size.is_empty() || (size.len() > 1 && size.starts_with('0')) || !size.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(WikiObjectError::Format);
    }
    let size: usize = size.parse().map_err(|_| WikiObjectError::Format)?;
    let (kind, native, limit) = match kind {
        "blob" => (WikiScannedKind::Blob, gix::objs::Kind::Blob, rbit_wikis::MAX_WIKI_CONTENT_BYTES),
        "tree" => (WikiScannedKind::Tree, gix::objs::Kind::Tree, MAX_SCAN_OBJECT_BYTES as usize),
        "commit" => (WikiScannedKind::Commit, gix::objs::Kind::Commit, 64 * 1024),
        _ => return Err(WikiObjectError::Format),
    };
    let offset = end + 1;
    if size != data.len() - offset || size > limit {
        return Err(WikiObjectError::Format);
    }
    let actual = gix::objs::compute_hash(gix::hash::Kind::Sha1, native, &data[offset..]).map_err(crate::GitError::backend)?;
    if actual != expected {
        return Err(WikiObjectError::Format);
    }
    Ok(Decoded { data, offset, kind })
}
