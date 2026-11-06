//! Stable session persistence values; backend codecs stay in storage.

use crate::TokenDigest;
use rbit_contracts::{AccountId, Timestamp};
use serde::{Deserialize, Serialize};

const MAX_USER_AGENT_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub account_id: AccountId,
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
    /// Updated at most every few minutes, to keep reads write-free.
    pub last_seen_at: Timestamp,
    /// Client address at sign-in.
    pub ip_address: Option<String>,
    /// The browser's `User-Agent` at sign-in (truncated).
    pub user_agent: String,
}

impl SessionRecord {
    /// A session created now.
    pub fn new(account_id: AccountId, expires_at: Timestamp, ip_address: Option<String>, user_agent: &str) -> Self {
        Self::at(account_id, expires_at, ip_address, user_agent, Timestamp::now())
    }

    pub(crate) fn at(account_id: AccountId, expires_at: Timestamp, ip_address: Option<String>, user_agent: &str, now: Timestamp) -> Self {
        let end = user_agent.char_indices().nth(MAX_USER_AGENT_LEN).map_or(user_agent.len(), |(index, _)| index);
        Self { account_id, expires_at, created_at: now, last_seen_at: now, ip_address, user_agent: user_agent[..end].to_owned() }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at <= Timestamp::now()
    }
}

/// A session together with its key, as listed for its account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub digest: TokenDigest,
    pub record: SessionRecord,
}
