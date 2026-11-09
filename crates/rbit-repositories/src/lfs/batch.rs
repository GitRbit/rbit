//! Batch admission, per-object answers and the upload/verify checks.

use std::{collections::HashMap, future::Future, hash::Hash};

/// Most objects one batch request may name.
pub const MAX_BATCH_OBJECTS: usize = 1000;

/// Why a transfer is refused before any object is considered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LfsRefusal {
    Forbidden,
    Archived,
    /// A safe-to-show validation message.
    Invalid(String),
    /// The announced size exceeds the configured limit.
    TooLarge,
}

/// One object of a batch request, exactly as the client sent it.
#[derive(Debug, Clone, Copy)]
pub struct BatchItem<'a> {
    pub oid: &'a str,
    pub size: i64,
}

/// What the batch answers for one object (tokens are issued by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectDecision<O> {
    /// A malformed id or size.
    Invalid(String),
    /// Downloadable; `stored` is the stored size, which the response reports.
    Download { oid: O, stored: u64 },
    /// A download of an object this repository cannot use.
    Missing,
    /// No upload needed.
    Present,
    /// Larger than the configured limit.
    TooLarge,
    /// The client must upload `size` bytes.
    Upload { oid: O, size: u64 },
}

/// Object facts for one repository and requester.
pub trait BatchPort: Sync {
    type Oid: Clone + Eq + Hash + Send + Sync;
    type Error: Send;
    /// A valid object id, or the reason it is not one.
    fn parse_oid(&self, oid: &str) -> Result<Self::Oid, String>;
    /// Sizes of the `oids` linked to the repository (one query).
    fn linked(&self, oids: &[Self::Oid]) -> impl Future<Output = Result<HashMap<Self::Oid, u64>, Self::Error>> + Send;
    /// The size of `oid` if a readable repository of the fork network has it, after
    /// linking it here too. The readable network is read once per batch.
    fn through_network(&self, oid: &Self::Oid) -> impl Future<Output = Result<Option<u64>, Self::Error>> + Send;
    fn bytes_present(&self, oid: &Self::Oid) -> impl Future<Output = bool> + Send;
}

/// Uploads need write access to a repository that is not archived; every batch is bounded.
pub fn admit_transfer(write: bool, can_write: bool, archived: bool, objects: usize) -> Result<(), LfsRefusal> {
    if write {
        if !can_write {
            return Err(LfsRefusal::Forbidden);
        }
        if archived {
            return Err(LfsRefusal::Archived);
        }
    }
    if objects > MAX_BATCH_OBJECTS {
        return Err(LfsRefusal::Invalid(format!("a batch can name at most {MAX_BATCH_OBJECTS} objects")));
    }
    Ok(())
}

/// The answer for every object, in request order. Objects found through the fork
/// network count as linked for later occurrences in the same batch.
pub async fn decide_batch<P: BatchPort>(
    port: &P,
    write: bool,
    objects: &[BatchItem<'_>],
    max: u64,
) -> Result<Vec<ObjectDecision<P::Oid>>, P::Error> {
    let valid: Vec<P::Oid> = objects.iter().filter_map(|object| port.parse_oid(object.oid).ok()).collect();
    let mut linked = port.linked(&valid).await?;
    let mut answers = Vec::with_capacity(objects.len());
    for object in objects {
        let decision = match (port.parse_oid(object.oid), u64::try_from(object.size)) {
            (Err(error), _) => ObjectDecision::Invalid(error),
            (_, Err(_)) => ObjectDecision::Invalid("the size cannot be negative".to_owned()),
            (Ok(oid), Ok(size)) => {
                if !linked.contains_key(&oid)
                    && let Some(found) = port.through_network(&oid).await?
                {
                    linked.insert(oid.clone(), found);
                }
                match (write, linked.get(&oid).copied()) {
                    (false, Some(stored)) => ObjectDecision::Download { oid, stored },
                    (false, None) => ObjectDecision::Missing,
                    (true, Some(stored)) if stored == size && port.bytes_present(&oid).await => ObjectDecision::Present,
                    (true, _) if size > max => ObjectDecision::TooLarge,
                    (true, _) => ObjectDecision::Upload { oid, size },
                }
            }
        };
        answers.push(decision);
    }
    Ok(answers)
}

/// Storing needs write access, a repository that is not archived and an allowed size.
pub fn admit_store(can_write: bool, archived: bool, size: u64, max: u64) -> Result<(), LfsRefusal> {
    if !can_write {
        return Err(LfsRefusal::Forbidden);
    }
    if archived {
        return Err(LfsRefusal::Archived);
    }
    if size > max {
        return Err(LfsRefusal::TooLarge);
    }
    Ok(())
}

/// Why received bytes cannot be the announced object: `received` is `None` when more
/// than `size` bytes arrived, otherwise the received size and whether the digest
/// matches the object id.
pub fn upload_refusal(size: u64, received: Option<(u64, bool)>) -> Option<String> {
    match received {
        None => Some(format!("the upload is larger than the announced {size} bytes")),
        Some((received, _)) if received != size => Some(format!("received {received} bytes, but the object has {size}")),
        Some((_, false)) => Some("the content does not match the object id".to_owned()),
        Some(_) => None,
    }
}

/// The verify step's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    Verified,
    /// Not linked to the repository.
    Missing,
    /// Linked with another size; a safe-to-show message.
    Mismatch(String),
}

/// The verify step: the object is linked with exactly `size` bytes.
pub fn verify_stored(stored: Option<u64>, size: u64) -> Verification {
    match stored {
        Some(stored) if stored == size => Verification::Verified,
        Some(stored) => Verification::Mismatch(format!("the object has {stored} bytes, not {size}")),
        None => Verification::Missing,
    }
}
