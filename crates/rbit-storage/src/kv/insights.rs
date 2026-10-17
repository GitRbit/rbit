//! Cached repository insights: language statistics, last-commit-per-entry maps and the
//! counts of a comparison between two commits.
//!
//! All are pure functions of commit content, so entries are keyed by commit ids and never
//! go stale (a commit id names its whole history, so the same pair of ids compares the
//! same way in every repository of a fork network); a push simply produces new keys. Values are `postcard`-encoded. A value
//! that no longer decodes (after a format change) is treated as a miss and overwritten.
//!
//! Because keys never go stale the cache would only grow, so every write also records
//! *when* the entry was written, in two small bookkeeping tables: one keyed like the data
//! (to find an entry's age) and one ordered by age (to find the oldest entries without a
//! scan). [`InsightsCache::prune`] removes entries past a maximum age and then the oldest
//! entries beyond a maximum count; anything removed is simply recomputed on demand.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{StorageError, error::kv_error};

/// `commit id` → encoded [`CachedLanguageStats`].
const LANGUAGE_STATS: TableDefinition<'_, &str, &[u8]> = TableDefinition::new("language_stats");

/// `(commit id, directory path)` → encoded [`CachedLastCommits`].
const TREE_LAST_COMMIT: TableDefinition<'_, (&str, &str), &[u8]> = TableDefinition::new("tree_last_commit");

/// `(base commit id, head commit id)` → encoded [`CachedComparisonCounts`].
const COMPARISON_COUNTS: TableDefinition<'_, (&str, &str), &[u8]> = TableDefinition::new("comparison_counts");

/// `(entry kind, commit id, directory)` → Unix seconds when the entry was written.
const ENTRY_WRITTEN: TableDefinition<'_, (u8, &str, &str), u64> = TableDefinition::new("insights_entry_written");

/// `(written at, entry kind, commit id, directory)` → nothing: every entry, oldest first.
const ENTRIES_BY_AGE: TableDefinition<'_, (u64, u8, &str, &str), ()> = TableDefinition::new("insights_entries_by_age");

/// Which data table a bookkeeping row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    /// [`LANGUAGE_STATS`]; the directory is always empty.
    Languages = 0,
    /// [`TREE_LAST_COMMIT`].
    LastCommits = 1,
    /// [`COMPARISON_COUNTS`]; the "directory" is the head commit id.
    ComparisonCounts = 2,
}

impl EntryKind {
    fn tag(self) -> u8 {
        self as u8
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Languages),
            1 => Some(Self::LastCommits),
            2 => Some(Self::ComparisonCounts),
            _ => None,
        }
    }
}

/// Bounds for a prune; `None` means unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PruneLimits {
    max_age: Option<Duration>,
    max_entries: Option<u64>,
}

/// What a prune removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Entries older than the maximum age.
    pub expired: u64,
    /// The oldest entries beyond the maximum count.
    pub evicted: u64,
    /// Entries left afterwards.
    pub remaining: u64,
}

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

/// What a comparison of two commits counts, for headers that show the numbers without the
/// diff (pull request tabs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedComparisonCounts {
    /// Commits in head but not in base.
    pub ahead: u64,
    /// Changed files, as the diff listed them (bounded by the diff limits).
    pub files: u64,
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
        tx.open_table(COMPARISON_COUNTS).map_err(kv_error)?;
        tx.open_table(ENTRY_WRITTEN).map_err(kv_error)?;
        tx.open_table(ENTRIES_BY_AGE).map_err(kv_error)?;
        tx.commit().map_err(kv_error)
    }

    /// A trivial read for health checks: fails when the store cannot be read.
    pub async fn check(&self) -> Result<(), StorageError> {
        self.entry_count().await.map(drop)
    }

    /// Number of cached entries (language breakdowns and last-commit maps together).
    pub async fn entry_count(&self) -> Result<u64, StorageError> {
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            tx.open_table(ENTRY_WRITTEN).map_err(kv_error)?.len().map_err(kv_error)
        })
        .await?
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
            record_written(&tx, EntryKind::Languages, &key, "", unix_now())?;
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
            record_written(&tx, EntryKind::LastCommits, &commit_id, &directory, unix_now())?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// The counts of comparing `head` against `base` (both full commit ids), if cached.
    pub async fn comparison_counts(&self, base: &str, head: &str) -> Result<Option<CachedComparisonCounts>, StorageError> {
        let db = Arc::clone(&self.db);
        let (base, head) = (base.to_owned(), head.to_owned());
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_read().map_err(kv_error)?;
            let table = tx.open_table(COMPARISON_COUNTS).map_err(kv_error)?;
            Ok(table.get((base.as_str(), head.as_str())).map_err(kv_error)?.and_then(|value| decode(value.value())))
        })
        .await?
    }

    pub async fn put_comparison_counts(&self, base: &str, head: &str, counts: CachedComparisonCounts) -> Result<(), StorageError> {
        let db = Arc::clone(&self.db);
        let (base, head) = (base.to_owned(), head.to_owned());
        let value = encode(&counts)?;
        tokio::task::spawn_blocking(move || {
            let tx = db.begin_write().map_err(kv_error)?;
            tx.open_table(COMPARISON_COUNTS)
                .map_err(kv_error)?
                .insert((base.as_str(), head.as_str()), value.as_slice())
                .map_err(kv_error)?;
            record_written(&tx, EntryKind::ComparisonCounts, &base, &head, unix_now())?;
            tx.commit().map_err(kv_error)
        })
        .await?
    }

    /// Removes entries older than `max_age`, then the oldest entries beyond `max_entries`,
    /// in one write transaction. `None` leaves that bound out.
    pub async fn prune(&self, max_age: Option<Duration>, max_entries: Option<u64>) -> Result<PruneReport, StorageError> {
        let limits = PruneLimits { max_age, max_entries };
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || prune_at(&db, limits, unix_now())).await?
    }
}

/// Records that the entry `(kind, commit_id, directory)` was written at `now`, replacing
/// any earlier record so each entry appears exactly once in the age index.
fn record_written(tx: &WriteTransaction, kind: EntryKind, commit_id: &str, directory: &str, now: u64) -> Result<(), StorageError> {
    let tag = kind.tag();
    let mut written = tx.open_table(ENTRY_WRITTEN).map_err(kv_error)?;
    let mut by_age = tx.open_table(ENTRIES_BY_AGE).map_err(kv_error)?;
    let previous = written.insert((tag, commit_id, directory), now).map_err(kv_error)?.map(|previous| previous.value());
    if let Some(previous) = previous {
        by_age.remove((previous, tag, commit_id, directory)).map_err(kv_error)?;
    }
    by_age.insert((now, tag, commit_id, directory), ()).map_err(kv_error)?;
    Ok(())
}

fn prune_at(db: &redb::Database, limits: PruneLimits, now: u64) -> Result<PruneReport, StorageError> {
    let tx = db.begin_write().map_err(kv_error)?;
    adopt_untracked_entries(&tx)?;

    let cutoff = limits.max_age.map(|age| now.saturating_sub(age.as_secs()));
    let mut report = PruneReport::default();
    {
        let mut by_age = tx.open_table(ENTRIES_BY_AGE).map_err(kv_error)?;
        let mut written = tx.open_table(ENTRY_WRITTEN).map_err(kv_error)?;
        let mut languages = tx.open_table(LANGUAGE_STATS).map_err(kv_error)?;
        let mut last_commits = tx.open_table(TREE_LAST_COMMIT).map_err(kv_error)?;
        let mut comparisons = tx.open_table(COMPARISON_COUNTS).map_err(kv_error)?;

        let mut remaining = by_age.len().map_err(kv_error)?;
        loop {
            let Some(oldest) = by_age.first().map_err(kv_error)?.map(|(key, _)| key.value().0) else { break };
            let expired = cutoff.is_some_and(|cutoff| oldest < cutoff);
            let over_capacity = limits.max_entries.is_some_and(|max| remaining > max);
            if !expired && !over_capacity {
                break;
            }

            let Some((key, _)) = by_age.pop_first().map_err(kv_error)? else { break };
            let (_, tag, commit_id, directory) = key.value();
            let (commit_id, directory) = (commit_id.to_owned(), directory.to_owned());
            drop(key);
            remaining -= 1;

            written.remove((tag, commit_id.as_str(), directory.as_str())).map_err(kv_error)?;
            match EntryKind::from_tag(tag) {
                Some(EntryKind::Languages) => {
                    languages.remove(commit_id.as_str()).map_err(kv_error)?;
                }
                Some(EntryKind::LastCommits) => {
                    last_commits.remove((commit_id.as_str(), directory.as_str())).map_err(kv_error)?;
                }
                Some(EntryKind::ComparisonCounts) => {
                    comparisons.remove((commit_id.as_str(), directory.as_str())).map_err(kv_error)?;
                }
                None => {}
            }
            if expired {
                report.expired += 1;
            } else {
                report.evicted += 1;
            }
        }
        report.remaining = remaining;
    }
    tx.commit().map_err(kv_error)?;
    Ok(report)
}

/// Entries written before age tracking existed have no bookkeeping rows. They are
/// recorded as written at the epoch, so they are the first to be pruned.
fn adopt_untracked_entries(tx: &WriteTransaction) -> Result<(), StorageError> {
    let mut untracked = Vec::new();
    {
        let written = tx.open_table(ENTRY_WRITTEN).map_err(kv_error)?;
        let languages = tx.open_table(LANGUAGE_STATS).map_err(kv_error)?;
        let last_commits = tx.open_table(TREE_LAST_COMMIT).map_err(kv_error)?;
        // Comparison counts never existed untracked, but count towards the bookkeeping rows.
        let comparisons = tx.open_table(COMPARISON_COUNTS).map_err(kv_error)?.len().map_err(kv_error)?;
        let stored = languages.len().map_err(kv_error)? + last_commits.len().map_err(kv_error)? + comparisons;
        if stored <= written.len().map_err(kv_error)? {
            return Ok(());
        }

        for entry in languages.iter().map_err(kv_error)? {
            let (key, _) = entry.map_err(kv_error)?;
            let commit_id = key.value();
            if written.get((EntryKind::Languages.tag(), commit_id, "")).map_err(kv_error)?.is_none() {
                untracked.push((EntryKind::Languages, commit_id.to_owned(), String::new()));
            }
        }
        for entry in last_commits.iter().map_err(kv_error)? {
            let (key, _) = entry.map_err(kv_error)?;
            let (commit_id, directory) = key.value();
            if written.get((EntryKind::LastCommits.tag(), commit_id, directory)).map_err(kv_error)?.is_none() {
                untracked.push((EntryKind::LastCommits, commit_id.to_owned(), directory.to_owned()));
            }
        }
    }
    for (kind, commit_id, directory) in untracked {
        record_written(tx, kind, &commit_id, &directory, 0)?;
    }
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
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

    fn stats() -> CachedLanguageStats {
        CachedLanguageStats { total_bytes: 1, languages: Vec::new() }
    }

    fn open_cache(dir: &tempfile::TempDir) -> InsightsCache {
        KvStore::open(&dir.path().join("kv.redb")).unwrap().insights()
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
        let cache = open_cache(&dir);

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

        let counts = CachedComparisonCounts { ahead: 3, files: 12 };
        assert_eq!(cache.comparison_counts("b1", "h1").await.unwrap(), None);
        cache.put_comparison_counts("b1", "h1", counts).await.unwrap();
        assert_eq!(cache.comparison_counts("b1", "h1").await.unwrap(), Some(counts));
        assert_eq!(cache.comparison_counts("h1", "b1").await.unwrap(), None, "ordered pair");
    }

    #[tokio::test]
    async fn prune_removes_comparison_counts() {
        let dir = tempfile::tempdir().unwrap();
        let cache = open_cache(&dir);
        cache.put_comparison_counts("b1", "h1", CachedComparisonCounts { ahead: 1, files: 1 }).await.unwrap();
        cache.put_language_stats("c1", &stats()).await.unwrap();
        assert_eq!(cache.entry_count().await.unwrap(), 2);
        let report = cache.prune(None, Some(0)).await.unwrap();
        assert_eq!(report, PruneReport { expired: 0, evicted: 2, remaining: 0 });
        assert_eq!(cache.comparison_counts("b1", "h1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn rewriting_an_entry_keeps_one_age_record() {
        let dir = tempfile::tempdir().unwrap();
        let cache = open_cache(&dir);
        cache.put_language_stats("c1", &stats()).await.unwrap();
        cache.put_language_stats("c1", &stats()).await.unwrap();
        cache.put_last_commits("c1", "", &CachedLastCommits::default()).await.unwrap();
        assert_eq!(cache.entry_count().await.unwrap(), 2);
        let tx = cache.db.begin_read().unwrap();
        assert_eq!(tx.open_table(ENTRIES_BY_AGE).unwrap().len().unwrap(), 2);
    }

    #[tokio::test]
    async fn prune_removes_expired_then_oldest_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = open_cache(&dir);
        let tx = cache.db.begin_write().unwrap();
        for (written_at, commit) in [(100, "a"), (200, "b"), (300, "c"), (400, "d")] {
            tx.open_table(LANGUAGE_STATS).unwrap().insert(commit, encode(&stats()).unwrap().as_slice()).unwrap();
            record_written(&tx, EntryKind::Languages, commit, "", written_at).unwrap();
        }
        let empty = encode(&CachedLastCommits::default()).unwrap();
        tx.open_table(TREE_LAST_COMMIT).unwrap().insert(("d", "src"), empty.as_slice()).unwrap();
        record_written(&tx, EntryKind::LastCommits, "d", "src", 400).unwrap();
        tx.commit().unwrap();

        // Cut-off at 250 expires `a` and `b`; a cap of two then evicts `c`, the oldest left.
        let limits = PruneLimits { max_age: Some(Duration::from_secs(250)), max_entries: Some(2) };
        let report = prune_at(&cache.db, limits, 500).unwrap();
        assert_eq!(report, PruneReport { expired: 2, evicted: 1, remaining: 2 });
        assert_eq!(cache.language_stats("a").await.unwrap(), None);
        assert_eq!(cache.language_stats("c").await.unwrap(), None);
        assert_eq!(cache.language_stats("d").await.unwrap(), Some(stats()));
        assert!(cache.last_commits("d", "src").await.unwrap().is_some());
        assert_eq!(cache.entry_count().await.unwrap(), 2);

        let unbounded = prune_at(&cache.db, PruneLimits::default(), 10_000).unwrap();
        assert_eq!(unbounded, PruneReport { expired: 0, evicted: 0, remaining: 2 });
    }

    #[tokio::test]
    async fn prune_adopts_entries_written_before_age_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let cache = open_cache(&dir);
        let tx = cache.db.begin_write().unwrap();
        tx.open_table(LANGUAGE_STATS).unwrap().insert("legacy", encode(&stats()).unwrap().as_slice()).unwrap();
        tx.commit().unwrap();
        cache.put_language_stats("fresh", &stats()).await.unwrap();

        let report = cache.prune(Some(Duration::from_secs(60)), None).await.unwrap();
        assert_eq!(report, PruneReport { expired: 1, evicted: 0, remaining: 1 });
        assert_eq!(cache.language_stats("legacy").await.unwrap(), None);
        assert!(cache.language_stats("fresh").await.unwrap().is_some());
        cache.check().await.unwrap();
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
