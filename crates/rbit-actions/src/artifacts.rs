//! The artifact store (§12.7): files a job uploads, one tar.gz each, for later jobs of the
//! run and for people to download.
//!
//! * An upload is refused unless its name is valid and its digest is 64 hex digits, its
//!   task holds its job's lease, and the repository has room: the smaller of the instance's
//!   `max_artifact_bytes` and what `max_artifact_storage_bytes` leaves. The body must match
//!   the digest, and the lease must still hold after it, before the row is recorded.
//! * Names are unique per run attempt; `overwrite` replaces, and the replaced file goes
//!   before the new one is placed (the new row may reuse its id, and so its path).
//! * An artifact is kept `artifact_retention_days`, or fewer when the workflow asks; the
//!   sweep removes expired ones with their files.

use std::{collections::BTreeSet, future::Future};

use rbit_workflow::protocol::{sha256_from_hex, valid_artifact_name};

use crate::archives::{ArchiveFiles, ArchiveUploads, BATCH, Refusal, SECONDS_PER_DAY, StoreError};

/// The instance's artifact limits (`[actions]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactLimits {
    pub max_artifact_bytes: u64,
    pub max_storage_bytes: u64,
    pub retention_days: u32,
}

/// What a runner sends with an upload.
#[derive(Debug, Clone, Copy)]
pub struct ArtifactUpload<'a> {
    pub name: &'a str,
    /// The tar.gz's SHA-256, as 64 hex digits.
    pub sha256: &'a str,
    /// `retention-days`; it can only lower the instance's.
    pub retention_days: Option<u32>,
    pub overwrite: bool,
}

/// The row of an upload, for the store to record.
#[derive(Debug)]
pub struct NewArtifact<'a, T> {
    /// The uploading task's run and job, as the lease found them.
    pub target: &'a T,
    pub name: &'a str,
    pub size: u64,
    pub sha256: [u8; 32],
    pub now: i64,
    pub expires_at: i64,
}

/// What recording an upload did.
#[derive(Debug)]
pub enum ArtifactRecorded<A> {
    Stored {
        artifact: A,
        replaced: Option<A>,
    },
    /// The run attempt has an artifact of the name, and `overwrite` was not set.
    Exists,
}

/// Stored artifacts: their files and rows.
pub trait ArtifactRecords: ArchiveFiles {
    type Artifact: Send + Sync;

    fn location(&self, artifact: &Self::Artifact) -> Self::Location;
    fn delete(&self, artifact: &Self::Artifact) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// One task's upload: its lease, its repository's usage and the row.
pub trait ArtifactUploadStore: ArtifactRecords + ArchiveUploads {
    type Target: Send + Sync;

    /// The uploading task's run and job, while it holds its job's lease.
    fn lease(&self) -> impl Future<Output = Result<Self::Target, Self::Error>> + Send;
    /// The lease again, after the body: it may have ended meanwhile.
    fn still_leased(&self, target: &Self::Target) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Bytes the target's repository keeps in artifacts.
    fn stored_bytes(&self, target: &Self::Target) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    /// Unix seconds now.
    fn now(&self) -> i64;
    fn record(
        &self,
        new: &NewArtifact<'_, Self::Target>,
        overwrite: bool,
    ) -> impl Future<Output = Result<ArtifactRecorded<Self::Artifact>, Self::Error>> + Send;
}

/// Expired artifacts, for the sweep.
pub trait ArtifactRetention: ArtifactRecords {
    fn expired(&self, now: i64, limit: u32) -> impl Future<Output = Result<Vec<Self::Artifact>, Self::Error>> + Send;
}

/// The days an artifact is kept: the workflow's `retention-days`, within 1 and the
/// instance's, else the instance's.
pub fn retention_days(asked: Option<u32>, instance: u32) -> u32 {
    asked.map_or(instance, |days| days.clamp(1, instance))
}

/// The newest attempt's artifact of each name, by name, from a list newest first.
pub fn latest_per_name<A>(artifacts: Vec<A>, name: impl Fn(&A) -> &str) -> Vec<A> {
    let mut seen = BTreeSet::new();
    let mut latest: Vec<A> = artifacts.into_iter().filter(|artifact| seen.insert(name(artifact).to_owned())).collect();
    latest.sort_by(|a, b| name(a).cmp(name(b)));
    latest
}

/// Uploads, removals and expiry of the artifact store.
#[derive(Debug)]
pub struct ArtifactStorage;

impl ArtifactStorage {
    /// Stores `body` as artifact `upload.name` of the leased task's run.
    pub async fn upload<P: ArtifactUploadStore>(
        port: &P,
        limits: &ArtifactLimits,
        upload: &ArtifactUpload<'_>,
        body: P::Body,
    ) -> Result<P::Artifact, StoreError<P::Error>> {
        if !valid_artifact_name(upload.name) {
            return Err(StoreError::bad_request("artifact names are 1 to 128 letters, digits, '.', '_' or '-', and not '.' or '..'"));
        }
        let expected = sha256_from_hex(upload.sha256).ok_or_else(|| StoreError::bad_request("the content digest must be 64 hex digits"))?;
        let target = port.lease().await.map_err(StoreError::Port)?;
        let stored = port.stored_bytes(&target).await.map_err(StoreError::Port)?;
        let room = limits.max_storage_bytes.saturating_sub(stored).min(limits.max_artifact_bytes);
        if room == 0 {
            return Err(StoreError::Refused(Refusal::TooLarge("the repository's artifact storage is full".into())));
        }
        let Some(received) = port.receive(body, room).await.map_err(StoreError::Port)? else {
            let message = if room < limits.max_artifact_bytes {
                "the artifact does not fit in the repository's artifact storage"
            } else {
                "the artifact is larger than this instance allows"
            };
            return Err(StoreError::Refused(Refusal::TooLarge(message.into())));
        };
        if received.sha256 != expected {
            port.discard(&received.temporary).await;
            return Err(StoreError::bad_request("the artifact does not match its content digest"));
        }
        // The body may have taken an hour: the lease may have ended meanwhile.
        if let Err(error) = port.still_leased(&target).await {
            port.discard(&received.temporary).await;
            return Err(StoreError::Port(error));
        }
        let now = port.now();
        let days = retention_days(upload.retention_days, limits.retention_days);
        let new = NewArtifact {
            target: &target,
            name: upload.name,
            size: received.size,
            sha256: received.sha256,
            now,
            expires_at: now + i64::from(days) * SECONDS_PER_DAY,
        };
        let (artifact, replaced) = match port.record(&new, upload.overwrite).await.map_err(StoreError::Port)? {
            ArtifactRecorded::Stored { artifact, replaced } => (artifact, replaced),
            ArtifactRecorded::Exists => {
                port.discard(&received.temporary).await;
                return Err(StoreError::Refused(Refusal::Conflict(format!(
                    "this run already has an artifact `{}`; set `overwrite: true` to replace it",
                    upload.name
                ))));
            }
        };
        // The replaced file goes first: the new row may have the old one's id, and so the
        // same path.
        if let Some(replaced) = &replaced {
            port.remove(&port.location(replaced)).await;
        }
        if let Err(error) = port.place(&received.temporary, &port.location(&artifact)).await {
            port.discard(&received.temporary).await;
            port.delete(&artifact).await.map_err(StoreError::Port)?;
            return Err(StoreError::Port(error));
        }
        Ok(artifact)
    }

    /// Deletes `artifact`'s file, then its row.
    pub async fn remove<P: ArtifactRecords>(port: &P, artifact: &P::Artifact) -> Result<(), P::Error> {
        port.remove(&port.location(artifact)).await;
        port.delete(artifact).await
    }

    /// Deletes up to [`BATCH`] artifacts that expired by `now`, with their files; how many.
    pub async fn expire<P: ArtifactRetention>(port: &P, now: i64) -> Result<usize, P::Error> {
        let expired = port.expired(now, BATCH).await?;
        for artifact in &expired {
            Self::remove(port, artifact).await?;
        }
        Ok(expired.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_only_lowers_the_instance_days() {
        assert_eq!(retention_days(None, 90), 90);
        assert_eq!(retention_days(Some(5), 90), 5);
        assert_eq!(retention_days(Some(0), 90), 1);
        assert_eq!(retention_days(Some(400), 90), 90);
    }

    #[test]
    fn the_newest_attempt_wins_and_names_sort() {
        let latest = latest_per_name(vec![("b", 2), ("a", 2), ("b", 1)], |artifact| artifact.0);
        assert_eq!(latest, [("a", 2), ("b", 2)]);
    }
}
