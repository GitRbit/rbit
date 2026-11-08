//! Compare exact kind/wire bytes before any write into an existing object ID.
use super::{WikiObjectError, layout};
use gix::objs::WriteTo;
use std::io::{self, Write};

struct Compare<'a> {
    remaining: &'a [u8],
}
impl Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.remaining.starts_with(bytes) {
            return Err(io::Error::other("wiki object bytes differ"));
        }
        self.remaining = &self.remaining[bytes.len()..];
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn expected_id(repo: &gix::Repository, object: &impl WriteTo) -> Result<gix::ObjectId, WikiObjectError> {
    let mut writer = gix::hash::io::Write::new(io::sink(), repo.object_hash());
    writer.write_all(&object.loose_header()).map_err(crate::GitError::from)?;
    object.write_to(&mut writer).map_err(crate::GitError::from)?;
    writer.hash.try_finalize().map_err(|error| crate::GitError::backend(error).into())
}

fn present(repo: &gix::Repository, id: gix::ObjectId, object: &impl WriteTo) -> Result<bool, WikiObjectError> {
    let Some(header) = repo.try_find_header(id).map_err(crate::GitError::backend)? else { return Ok(false) };
    if header.kind() != object.kind() || header.size() != object.size() {
        return Err(WikiObjectError::Format);
    }
    let actual = repo.find_object(id).map_err(crate::GitError::backend)?;
    if actual.kind != object.kind() || u64::try_from(actual.data.len()).ok() != Some(object.size()) {
        return Err(WikiObjectError::Format);
    }
    let mut compare = Compare { remaining: &actual.data };
    object.write_to(&mut compare).map_err(|_| WikiObjectError::Format)?;
    if !compare.remaining.is_empty() {
        return Err(WikiObjectError::Format);
    }
    Ok(true)
}

/// A loose object lands in `objects/<first two hex digits>`; an observed link there (or a
/// Windows junction) would place it outside the wiki root, so it is refused first.
fn fan_out_without_links(repo: &gix::Repository, id: gix::ObjectId) -> Result<(), WikiObjectError> {
    let hex = id.to_hex().to_string();
    layout::no_links(&repo.git_dir().join("objects").join(&hex[..2]))
}

/// Caller MUST hold the cooperating root lock and exclusive wiki scope across the
/// absent check and write. This prevents the low-level persist replacement race.
/// Existing equal objects are reused without calling any gix write method.
pub(super) fn object(repo: &gix::Repository, object: impl WriteTo) -> Result<gix::ObjectId, WikiObjectError> {
    let id = expected_id(repo, &object)?;
    if present(repo, id, &object)? {
        return Ok(id);
    }
    // Same single reusable wire buffer as Repository::write_object; never clone
    // the entry vector or rehash the already derived ID. Only deterministic native
    // Tree/Commit encoders reach this private function.
    let mut encoded = repo.empty_reusable_buffer();
    object.write_to(&mut *encoded).map_err(crate::GitError::from)?;
    if u64::try_from(encoded.len()).ok() != Some(object.size()) {
        return Err(WikiObjectError::Format);
    }
    fan_out_without_links(repo, id)?;
    let written = gix::objs::Write::write_buf_with_known_id(&repo.objects, object.kind(), encoded.as_slice(), id)
        .map_err(crate::GitError::backend)?;
    // Return the wire buffer before read-back, so the repository can reuse it.
    drop(encoded);
    if written != id || !present(repo, id, &object)? {
        return Err(WikiObjectError::Format);
    }
    Ok(id)
}

/// Preserve the existing direct borrowed-blob path instead of serializing/copying
/// the body into `write_object`'s reusable buffer. The precomputed ID avoids a rehash.
pub(super) fn blob(repo: &gix::Repository, bytes: &[u8]) -> Result<gix::ObjectId, WikiObjectError> {
    let object = gix::objs::BlobRef { data: bytes };
    let id = gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, bytes).map_err(crate::GitError::backend)?;
    if present(repo, id, &object)? {
        return Ok(id);
    }
    fan_out_without_links(repo, id)?;
    let written =
        gix::objs::Write::write_buf_with_known_id(&repo.objects, gix::objs::Kind::Blob, bytes, id).map_err(crate::GitError::backend)?;
    if written != id || !present(repo, id, &object)? {
        return Err(WikiObjectError::Format);
    }
    Ok(id)
}
