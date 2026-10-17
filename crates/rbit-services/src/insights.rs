//! Repository insights: language breakdown and last commit per tree entry.
//!
//! Both are derived from Git alone and cached in redb by commit id, so a cached value is
//! valid forever and nothing needs invalidating on push. The cache is an optimisation
//! only: if it cannot be read or written, the value is computed and served anyway.

use std::collections::HashMap;

use rbit_core::Timestamp;
use rbit_git::{CommitSummary, LanguageShare, LanguageStats, LastCommitLimits, TreeEntry};
use rbit_storage::kv::{CachedCommit, CachedLanguage, CachedLanguageStats, CachedLastCommits, InsightsCache};

use crate::{RepoContext, ServiceContext, ServiceError};

#[derive(Debug, Clone)]
pub struct InsightsService {
    cache: InsightsCache,
}

impl InsightsService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { kv, .. } = ctx;
        Self { cache: kv.insights() }
    }

    /// The language breakdown of `revision` (a branch, tag or commit id).
    pub async fn languages(&self, repo: &RepoContext, revision: &str) -> Result<LanguageStats, ServiceError> {
        let commit_id = repo.reader.commit_id(revision.to_owned()).await?;
        match self.cache.language_stats(&commit_id).await {
            Ok(Some(cached)) => return Ok(language_stats_from_cache(cached)),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "language cache read failed"),
        }

        let stats = repo.reader.language_stats(commit_id.clone()).await?;
        if let Err(error) = self.cache.put_language_stats(&commit_id, &language_stats_to_cache(&stats)).await {
            tracing::warn!(%error, "language cache write failed");
        }
        Ok(stats)
    }

    /// The newest commit that changed each of `entries` inside the directory `path` as of
    /// `revision`. Entries the bounded history walk could not resolve are absent.
    pub async fn last_commits(
        &self,
        repo: &RepoContext,
        revision: &str,
        path: &str,
        entries: &[TreeEntry],
    ) -> Result<HashMap<String, CommitSummary>, ServiceError> {
        let commit_id = repo.reader.commit_id(revision.to_owned()).await?;
        match self.cache.last_commits(&commit_id, path).await {
            Ok(Some(cached)) => {
                let mut commits = last_commits_from_cache(&cached);
                commits.retain(|name, _| entries.iter().any(|entry| &entry.name == name));
                return Ok(commits);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "last-commit cache read failed"),
        }

        let names = entries.iter().map(|entry| entry.name.clone()).collect();
        let result = repo.reader.last_commits(commit_id.clone(), path.to_owned(), names, LastCommitLimits::default()).await?;
        // A walk cut short by the deadline depends on server load; try again next time.
        if !result.timed_out {
            let cached: Vec<(String, CachedCommit)> =
                result.commits.iter().map(|(name, commit)| (name.clone(), commit_to_cache(commit))).collect();
            let cached = CachedLastCommits::from_entries(cached.iter().map(|(name, commit)| (name.clone(), commit)));
            if let Err(error) = self.cache.put_last_commits(&commit_id, path, &cached).await {
                tracing::warn!(%error, "last-commit cache write failed");
            }
        }
        Ok(result.commits)
    }
}

fn language_stats_to_cache(stats: &LanguageStats) -> CachedLanguageStats {
    CachedLanguageStats {
        total_bytes: stats.total_bytes,
        languages: stats
            .languages
            .iter()
            .map(|share| CachedLanguage {
                name: share.name.clone(),
                color: share.color.clone(),
                bytes: share.bytes,
                percent: share.percent,
            })
            .collect(),
    }
}

fn language_stats_from_cache(cached: CachedLanguageStats) -> LanguageStats {
    LanguageStats {
        total_bytes: cached.total_bytes,
        languages: cached
            .languages
            .into_iter()
            .map(|language| LanguageShare { name: language.name, color: language.color, bytes: language.bytes, percent: language.percent })
            .collect(),
    }
}

fn commit_to_cache(commit: &CommitSummary) -> CachedCommit {
    CachedCommit {
        id: commit.id.clone(),
        summary: commit.summary.clone(),
        body: commit.body.clone(),
        author_name: commit.author_name.clone(),
        author_email: commit.author_email.clone(),
        authored_at: commit.authored_at.0,
        parent_ids: commit.parent_ids.clone(),
    }
}

fn commit_from_cache(commit: &CachedCommit) -> CommitSummary {
    CommitSummary {
        id: commit.id.clone(),
        short_id: commit.id.chars().take(7).collect(),
        summary: commit.summary.clone(),
        body: commit.body.clone(),
        author_name: commit.author_name.clone(),
        author_email: commit.author_email.clone(),
        authored_at: Timestamp(commit.authored_at),
        parent_ids: commit.parent_ids.clone(),
    }
}

fn last_commits_from_cache(cached: &CachedLastCommits) -> HashMap<String, CommitSummary> {
    // Convert each distinct commit once, then clone per entry.
    let commits: Vec<CommitSummary> = cached.commits.iter().map(commit_from_cache).collect();
    cached.entries.iter().filter_map(|(name, index)| Some((name.clone(), commits.get(usize::try_from(*index).ok()?)?.clone()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_survive_the_cache_format() {
        let commit = CommitSummary {
            id: "0123456789abcdef0123456789abcdef01234567".into(),
            short_id: "0123456".into(),
            summary: "Fix it".into(),
            body: Some("Details".into()),
            author_name: "Ada".into(),
            author_email: "ada@example.com".into(),
            authored_at: Timestamp(1_700_000_000),
            parent_ids: vec!["fedcba9876543210fedcba9876543210fedcba98".into()],
        };
        let restored = commit_from_cache(&commit_to_cache(&commit));
        assert_eq!(format!("{restored:?}"), format!("{commit:?}"));
    }

    #[test]
    fn language_stats_survive_the_cache_format() {
        let stats = LanguageStats {
            total_bytes: 100,
            languages: vec![LanguageShare { name: "Rust".into(), color: "#dea584".into(), bytes: 100, percent: 100.0 }],
        };
        assert_eq!(language_stats_from_cache(language_stats_to_cache(&stats)), stats);
    }
}
