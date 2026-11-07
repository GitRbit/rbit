//! Keep at most one raw tree, emitting ordered edge facts without a second tree Vec.
use super::{WikiObjectError, loose::Decoded};
use rbit_core::{AccountId, Timestamp};
use rbit_wikis::{
    WikiPageId, WikiRevisionId,
    startup::objects::{WikiScanRecord, WikiScannedKind},
};

pub(super) enum Pending {
    // Boxed so each retained scan state stays small; at most one is pending.
    Small { records: Box<[Option<WikiScanRecord>; 6]>, next: usize },
    Tree { id: WikiRevisionId, data: Vec<u8>, offset: usize, index: u32, previous: Vec<u8>, started: bool },
}
fn revision(id: gix::ObjectId) -> Result<WikiRevisionId, WikiObjectError> {
    WikiRevisionId::try_from(id.as_slice()).map_err(|_| WikiObjectError::Format)
}
fn number(value: &str) -> Result<i64, WikiObjectError> {
    if value.is_empty() || value.starts_with('0') || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(WikiObjectError::Format);
    }
    value.parse().ok().filter(|value| *value > 0).ok_or(WikiObjectError::Format)
}
impl Pending {
    pub(super) fn new(id: gix::ObjectId, decoded: Decoded) -> Result<Self, WikiObjectError> {
        let id = revision(id)?;
        let bytes = &decoded.data[decoded.offset..];
        let object =
            WikiScanRecord::Object { id, kind: decoded.kind, bytes: u32::try_from(bytes.len()).map_err(|_| WikiObjectError::Format)? };
        let complete = WikiScanRecord::Complete { id };
        match decoded.kind {
            WikiScannedKind::Blob => {
                std::str::from_utf8(bytes).map_err(|_| WikiObjectError::Format)?;
                if bytes.contains(&0) {
                    return Err(WikiObjectError::Format);
                }
                Ok(Self::Small { records: Box::new([Some(object), Some(complete), None, None, None, None]), next: 0 })
            }
            WikiScannedKind::Tree => {
                Ok(Self::Tree { id, data: decoded.data, offset: decoded.offset, index: 0, previous: Vec::new(), started: false })
            }
            WikiScannedKind::Commit => {
                let commit = gix::objs::CommitRef::from_bytes(bytes, gix::hash::Kind::Sha1).map_err(crate::GitError::backend)?;
                let message: &[u8] = commit.message.as_ref();
                if commit.parents.len() > 1
                    || commit.encoding.is_some()
                    || !commit.extra_headers.is_empty()
                    || message != b"Wiki page update\n"
                    || commit.author != commit.committer
                {
                    return Err(WikiObjectError::Format);
                }
                let author = commit.author().map_err(crate::GitError::backend)?;
                let name: &[u8] = author.name.as_ref();
                let label = std::str::from_utf8(name)
                    .map_err(|_| WikiObjectError::Format)?
                    .strip_prefix("account-")
                    .ok_or(WikiObjectError::Format)?;
                let (actor, incarnation) = match label.split_once("-wiki-actor-") {
                    Some((actor, incarnation)) => (number(actor)?, Some(number(incarnation)?)),
                    None => (number(label)?, None),
                };
                let email: &[u8] = author.email.as_ref();
                let time = author.time().map_err(crate::GitError::backend)?;
                if email != b"wiki@localhost" || time.offset != 0 {
                    return Err(WikiObjectError::Format);
                }
                let metadata = WikiScanRecord::Commit { id, actor: AccountId(actor), incarnation, at: Timestamp(time.seconds) };
                let tree = WikiScanRecord::Edge {
                    source: id,
                    ordinal: 0,
                    target: revision(commit.tree())?,
                    kind: WikiScannedKind::Tree,
                    page: None,
                };
                let parent = commit
                    .parents()
                    .next()
                    .map(|parent| {
                        revision(parent).map(|target| WikiScanRecord::Edge {
                            source: id,
                            ordinal: 1,
                            target,
                            kind: WikiScannedKind::Commit,
                            page: None,
                        })
                    })
                    .transpose()?;
                Ok(Self::Small { records: Box::new([Some(object), Some(metadata), Some(tree), parent, Some(complete), None]), next: 0 })
            }
        }
    }
    pub(super) fn next(&mut self) -> Result<Option<WikiScanRecord>, WikiObjectError> {
        match self {
            Self::Small { records, next } => {
                while let Some(record) = records.get_mut(*next) {
                    *next += 1;
                    if let Some(record) = record.take() {
                        return Ok(Some(record));
                    }
                }
                Ok(None)
            }
            Self::Tree { id, data, offset, index, previous, started } => {
                if *offset == usize::MAX {
                    return Ok(None);
                }
                if !*started {
                    *started = true;
                    return Ok(Some(WikiScanRecord::Object {
                        id: *id,
                        kind: WikiScannedKind::Tree,
                        bytes: u32::try_from(data.len() - *offset).map_err(|_| WikiObjectError::Format)?,
                    }));
                }
                let bytes = &data[*offset..];
                if bytes.is_empty() {
                    // A separate sentinel avoids retaining a second decoded tree.
                    *offset = usize::MAX;
                    return Ok(Some(WikiScanRecord::Complete { id: *id }));
                }
                if *index >= 10_000 || !bytes.starts_with(b"100644 ") {
                    return Err(WikiObjectError::Format);
                }
                let entry = gix::objs::TreeRefIter::from_bytes(bytes, gix::hash::Kind::Sha1)
                    .next()
                    .ok_or(WikiObjectError::Format)?
                    .map_err(crate::GitError::backend)?;
                let name: &[u8] = entry.filename.as_ref();
                let text = std::str::from_utf8(name).map_err(|_| WikiObjectError::Format)?;
                let page = text.strip_suffix(".md").ok_or(WikiObjectError::Format)?;
                let page = WikiPageId::new(number(page)?).ok_or(WikiObjectError::Format)?;
                if name.len() > 22 || (!previous.is_empty() && previous.as_slice() >= name) {
                    return Err(WikiObjectError::Format);
                }
                previous.clear();
                previous.extend_from_slice(name);
                *offset += 7 + name.len() + 1 + 20;
                let ordinal = *index;
                *index += 1;
                Ok(Some(WikiScanRecord::Edge {
                    source: *id,
                    ordinal,
                    target: revision(entry.oid.to_owned())?,
                    kind: WikiScannedKind::Blob,
                    page: Some(page),
                }))
            }
        }
    }
}
