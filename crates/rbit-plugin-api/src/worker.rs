//! Bounded native probe protocol values. No launch or broker authority.
//!
//! Native workers run as fully trusted service-OS-user code. Session nonces bind
//! a pipe conversation; they do not attest a binary or sandbox another process.
//! Complete-frame byte/depth/canonical checks belong to the codec before use.
use crate::{Generation, HostPolicyFingerprint, InstalledPackageRef, PolicyRevision, Sha256Digest};
use serde::{Deserialize, Serialize};

pub const WORKER_PROTOCOL_VERSION: u32 = 1;
pub const MAX_WORKER_FRAME_BYTES: usize = 65_536;
pub const MAX_WORKER_JSON_DEPTH: usize = 16;
pub const MAX_WORKER_SESSION_FRAMES: usize = 4096;
pub const MAX_WORKER_SESSION_BYTES: usize = 16 * 1024 * 1024;

/// Effective shared lifetime wire/work limits. Fixed v1 frame/depth ceilings
/// remain protocol constants; a worker cannot raise these host-selected budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WorkerLimitsWire")]
pub struct WorkerLimits {
    lifetime_frames: usize,
    lifetime_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerLimitsWire {
    lifetime_frames: usize,
    lifetime_bytes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid bounded worker lifetime limits")]
pub struct WorkerLimitsError;
impl WorkerLimits {
    pub fn new(frames: usize, bytes: usize) -> Result<Self, WorkerLimitsError> {
        if frames == 0 || frames > MAX_WORKER_SESSION_FRAMES || bytes == 0 || bytes > MAX_WORKER_SESSION_BYTES {
            return Err(WorkerLimitsError);
        }
        Ok(Self { lifetime_frames: frames, lifetime_bytes: bytes })
    }
    pub const fn frames(self) -> usize {
        self.lifetime_frames
    }
    pub const fn bytes(self) -> usize {
        self.lifetime_bytes
    }
}
impl Default for WorkerLimits {
    fn default() -> Self {
        Self { lifetime_frames: MAX_WORKER_SESSION_FRAMES, lifetime_bytes: MAX_WORKER_SESSION_BYTES }
    }
}
impl TryFrom<WorkerLimitsWire> for WorkerLimits {
    type Error = WorkerLimitsError;
    fn try_from(value: WorkerLimitsWire) -> Result<Self, Self::Error> {
        Self::new(value.lifetime_frames, value.lifetime_bytes)
    }
}

/// Host-assigned exact session identity, not a serializable execution permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerIdentity {
    pub package: InstalledPackageRef,
    pub epoch: Sha256Digest,
    pub host_policy: HostPolicyFingerprint,
    pub selection_revision: PolicyRevision,
    pub review_revision: PolicyRevision,
    pub host_instance: Sha256Digest,
    pub generation: Generation,
    pub launch_nonce: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
// One bounded identity stays inline instead of adding a separate handshake box.
#[allow(clippy::large_enum_variant)]
pub enum HostMessage {
    Hello { protocol_version: u32, identity: WorkerIdentity, limits: WorkerLimits },
    Ping { sequence: PolicyRevision },
    Stop {},
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerFailureCode {
    UnsupportedProtocol,
    Internal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
// One bounded identity stays inline instead of adding a separate handshake box.
#[allow(clippy::large_enum_variant)]
pub enum WorkerMessage {
    Ready { protocol_version: u32, identity: WorkerIdentity, limits: WorkerLimits },
    Pong { sequence: PolicyRevision },
    Stopped {},
    Error { code: WorkerFailureCode },
}
