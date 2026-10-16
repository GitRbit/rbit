//! Cached repository insights: language statistics and last-commit-per-entry maps.
//!
//! Both are pure functions of a commit's content, so entries are keyed by commit id and
//! never go stale; a push simply produces new keys. Values are `postcard`-encoded. A value
//! that no longer decodes (after a format change) is treated as a miss and overwritten.

use std::sync::Arc;

use redb::{ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{StorageError, error::kv_error};

/// `commit id` → encoded [`CachedLanguageStats`].
const LANGUAGE_STATS: TableDefinition<'_, &str, &[u8]> = TableDefinition::new("language_stats");

/// `(commit id, directory path)` → encoded [`CachedLastCommits`].
const TREE_LAST_COMMIT: TableDefinition<'_, (&str, &str), &[u8]> = TableDefinition::new("tree_last_commit");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedLanguage {
    pub name: String,
    pub color: String,
    pub bytes: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedLanguageStats {
    pub total_bytes: u64,
    pub languages: Vec<CachedLanguage>,
}

/// The commit fields needed to rebuild a commit summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedCommit {
    pub id: String,
    pub summary: String,
    pub body: Option<String>,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    pub authored_at: i64,
    pub parent_ids: Vec<String>,
}

/// Last commit per entry name. Many entries usually share a commit, so each distinct
/// commit is stored once and entries refer to it by index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedLastCommits {
    pub commits: Vec<CachedCommit>,
    /// `(entry name, index into commits)`.
    pub entries: Vec<(String, u32)>,
}

impl CachedLastCommits {
    /// Builds the deduplicated form from `(entry name, commit)` pairs.
    pub fn from_entries<'a>(entries: impl IntoIterator<Item = (String, &'a CachedCommit)>) -> Self {
        let mut cached = Self::default();
        for (name, commit) in entries {
            let index = cached.commits.iter().position(|known| known.id == commit.id).unwrap_or_else(|| {
                cached.commits.push(commit.clone());
                cached.commits.len() - 1
            });
            cached.entries.push((name, u32::try_from(index).unwrap_or(u32::MAX)));
        }
        cached
    }

    /// `(entry name, commit)` pairs; entries with a dangling index are skipped.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &CachedCommit)> {
        self.entries.iter().filter_map(|(name, index)| Some((name.as_str(), self.commits.get(usize::try_from(*index).ok()?)?)))
    }
}

#[derive(Debug, Clone)]
pub struct InsightsCache {
    db: Arc<redb::Database>,
}

impl InsightsCache {
    pub(crate) fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
    }

    /// Creates the tables so read transactions never fail on a fresh database.
    pub(crate) fn ensure_tables(&self) -> Result<(), StorageError> {
        let tx = self.db.begin_write().map_err(kv_error)?;
        tx.open_table(LANGUAGE_STATS).map_err(kv_error)?;
        tx.open_table(TREE_LAST_COMMIT).map_err(kv_error)?;
        tx.commit().map_err(kv_error)
    }

    pub async fn language_stats(&self, commit_id: &str) -> Result<Option<CachedLanguageStats>, StorageError> {
        let db = Arc::clone(&self.db);
        let key = commit_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(LANGUAGE_STATS).map_err(kv_error)?;
            Ok(table.get(key.as_str()).map_err(kv_error)?.and_then(|value| decode(value.value())))
        })
        .await?
    }

    pub async fn put_language_stats(&self, commit_id: &str, stats: &CachedLanguageStats) -> Result<(), StorageError> {
        let db = Arc::clone(&self.db);
        let key = commit_id.to_owned();
        let value = encode(stats)?;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            tx.open_table(LANGUAGE_STATS).map_err(kv_error)?.insert(key.as_str(), value.as_slice()).map_err(kv_error)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// `directory` is the slash-separated path inside the commit, empty for the root.
    pub async fn last_commits(&self, commit_id: &str, directory: &str) -> Result<Option<CachedLastCommits>, StorageError> {
        let db = Arc::clone(&self.db);
        let (commit_id, directory) = (commit_id.to_owned(), directory.to_owned());
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(TREE_LAST_COMMIT).map_err(kv_error)?;
            Ok(table.get((commit_id.as_str(), directory.as_str())).map_err(kv_error)?.and_then(|value| decode(value.value())))
        })
        .await?
    }

    pub async fn put_last_commits(&self, commit_id: &str, directory: &str, commits: &CachedLastCommits) -> Result<(), StorageError> {
        let db = Arc::clone(&self.db);
        let (commit_id, directory) = (commit_id.to_owned(), directory.to_owned());
        let value = encode(commits)?;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            tx.open_table(TREE_LAST_COMMIT)
                .map_err(kv_error)?
                .insert((commit_id.as_str(), directory.as_str()), value.as_slice())
                .map_err(kv_error)?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, StorageError> {
    postcard::to_stdvec(value).map_err(|error| StorageError::Corrupt(format!("cannot encode cache entry: {error}")))
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    match postcard::from_bytes(bytes) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::debug!(%error, "ignoring undecodable cache entry");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KvStore;

    fn commit(id: &str) -> CachedCommit {
        CachedCommit {
            id: id.to_owned(),
            summary: format!("commit {id}"),
            body: None,
            author_name: "Ada".into(),
            author_email: "ada@example.com".into(),
            authored_at: 1_700_000_000,
            parent_ids: vec!["p".into()],
        }
    }

    #[test]
    fn last_commits_share_commit_records() {
        let (a, b) = (commit("a"), commit("b"));
        let cached = CachedLastCommits::from_entries([("x".to_owned(), &a), ("y".to_owned(), &b), ("z".to_owned(), &a)]);
        assert_eq!(cached.commits.len(), 2);
        let pairs: Vec<_> = cached.iter().map(|(name, commit)| (name, commit.id.as_str())).collect();
        assert_eq!(pairs, [("x", "a"), ("y", "b"), ("z", "a")]);
    }

    #[tokio::test]
    async fn round_trips_through_redb() {
        let dir = tempfile::tempdir().unwrap();
        let cache = KvStore::open(&dir.path().join("kv.redb")).unwrap().insights();

        assert_eq!(cache.language_stats("c1").await.unwrap(), None);
        let stats = CachedLanguageStats {
            total_bytes: 10,
            languages: vec![CachedLanguage { name: "Rust".into(), color: "#dea584".into(), bytes: 10, percent: 100.0 }],
        };
        cache.put_language_stats("c1", &stats).await.unwrap();
        assert_eq!(cache.language_stats("c1").await.unwrap(), Some(stats));

        let last = CachedLastCommits::from_entries([("main.rs".to_owned(), &commit("a"))]);
        cache.put_last_commits("c1", "src", &last).await.unwrap();
        assert_eq!(cache.last_commits("c1", "src").await.unwrap(), Some(last));
        assert_eq!(cache.last_commits("c1", "").await.unwrap(), None, "keyed by directory too");
    }

    #[tokio::test]
    async fn undecodable_values_are_misses() {
        let dir = tempfile::tempdir().unwrap();
        let store = KvStore::open(&dir.path().join("kv.redb")).unwrap();
        let cache = store.insights();
        let tx = cache.db.begin_write().unwrap();
        tx.open_table(LANGUAGE_STATS).unwrap().insert("c1", [0xff_u8, 0xff, 0xff].as_slice()).unwrap();
        tx.commit().unwrap();
        assert_eq!(cache.language_stats("c1").await.unwrap(), None);
    }
}
