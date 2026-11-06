//! Session client metadata and the existing owner-facing projection.

use super::StoredSession;
use crate::TokenDigest;
use rbit_contracts::Timestamp;
use std::{fmt::Write as _, net::IpAddr};

const SESSION_ID_BYTES: usize = 12;

/// Where a request comes from, recorded with new sessions and audit entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientInfo {
    pub ip: Option<IpAddr>,
    /// The `User-Agent` header, empty when absent.
    pub user_agent: String,
}

/// A session as shown to its owner. Never carries the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSession {
    /// Public identifier for revocation: a prefix of the token's digest, hex-encoded.
    pub id: String,
    pub created_at: Timestamp,
    pub last_seen_at: Timestamp,
    pub expires_at: Timestamp,
    pub ip_address: Option<String>,
    pub user_agent: String,
    /// The session making the request.
    pub is_current: bool,
}

impl ActiveSession {
    pub fn from_stored(stored: StoredSession, current: Option<&TokenDigest>) -> Self {
        let StoredSession { digest, record } = stored;
        Self {
            id: session_id(&digest),
            created_at: record.created_at,
            last_seen_at: record.last_seen_at,
            expires_at: record.expires_at,
            ip_address: record.ip_address,
            user_agent: record.user_agent,
            is_current: current == Some(&digest),
        }
    }
}

pub fn session_id(digest: &TokenDigest) -> String {
    digest[..SESSION_ID_BYTES].iter().fold(String::with_capacity(SESSION_ID_BYTES * 2), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}
