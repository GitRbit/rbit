//! Visibility-filtered contribution counts derived from default-branch Git history.
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    sync::Arc,
};

use rbit_core::{Account, User};

use crate::{RepoContext, ServiceContext, ServiceError};

type Records = rbit_storage::kv::CachedContributions;
type HotRecords = VecDeque<((String, String), Arc<Records>)>;

#[derive(Debug, Default, Clone)]
pub struct Contributions {
    /// Unix day (UTC) to authored commit count.
    pub days: BTreeMap<i64, usize>,
    pub repositories: usize,
    pub partial: bool,
}

#[derive(Debug, Clone)]
pub struct ContributionService {
    ctx: ServiceContext,
    cache: rbit_storage::kv::InsightsCache,
    hot: Arc<tokio::sync::Mutex<HotRecords>>,
    scans: Arc<[tokio::sync::Mutex<()>; 16]>,
    workers: Arc<tokio::sync::Semaphore>,
}

impl ContributionService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            cache: ctx.kv.insights(),
            hot: Arc::default(),
            ctx,
            scans: Arc::new(std::array::from_fn(|_| tokio::sync::Mutex::new(()))),
            workers: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }

    /// Persist immutable records after visibility checks. Fixed coordination shards
    /// share same-tip misses; detached workers keep admission through cancellation.
    async fn history(&self, reader: &rbit_git::RepositoryReader, tip: String) -> Result<Arc<Records>, ServiceError> {
        // The traversal visits all ancestors regardless of date. Store that bounded
        // result once per tip so changing years or crossing midnight never rescans it.
        let window = "v2:all".to_owned();
        if let Some(records) = self.cached(&tip, &window).await {
            return Ok(records);
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        tip.hash(&mut hash);
        window.hash(&mut hash);
        let shard = (hash.finish() % 16) as usize;
        let service = self.clone();
        let reader = reader.clone();
        tokio::spawn(async move {
            let _scan = service.scans[shard].lock().await;
            if let Some(records) = service.cached(&tip, &window).await {
                return Ok(records);
            }
            let _worker = service.workers.acquire().await.map_err(ServiceError::internal)?;
            let records = Arc::new(reader.contributions(tip.clone(), i64::MIN, i64::MAX).await?);
            if let Err(error) = service.cache.put_contributions(&tip, &window, Arc::clone(&records)).await {
                tracing::warn!(%error, "contribution cache write failed");
            }
            service.remember(&tip, &window, Arc::clone(&records)).await;
            Ok(records)
        })
        .await
        .map_err(ServiceError::internal)?
    }

    async fn cached(&self, tip: &str, policy: &str) -> Option<Arc<Records>> {
        {
            let mut hot = self.hot.lock().await;
            if let Some(index) = hot.iter().position(|((id, version), _)| id == tip && version == policy) {
                let entry = hot.remove(index).expect("known entry");
                let records = Arc::clone(&entry.1);
                hot.push_back(entry);
                return Some(records);
            }
        }
        match self.cache.contributions(tip, policy).await {
            Ok(Some(records)) => {
                let records = Arc::new(records);
                self.remember(tip, policy, Arc::clone(&records)).await;
                Some(records)
            }
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, "contribution cache read failed");
                None
            }
        }
    }

    async fn remember(&self, tip: &str, policy: &str, records: Arc<Records>) {
        let mut hot = self.hot.lock().await;
        hot.retain(|((id, version), _)| id != tip || version != policy);
        while hot.len() >= 32 || hot.iter().map(|(_, value)| value.0.len()).sum::<usize>() + records.0.len() > 250_000 {
            if hot.pop_front().is_none() {
                return;
            }
        }
        hot.push_back(((tip.to_owned(), policy.to_owned()), records));
    }

    pub async fn account(&self, viewer: Option<&User>, account: &Account, start: i64, end: i64) -> Result<Contributions, ServiceError> {
        let store = self.ctx.database.repositories();
        let repos = if account.is_organization() {
            store.list_by_owner(account.id, viewer.map(User::id), viewer.is_some_and(|u| u.is_admin)).await?
        } else {
            store.contribution_repositories(viewer.map(User::id), viewer.is_some_and(|u| u.is_admin)).await?
        };
        let email = if account.is_organization() {
            None
        } else {
            Some(self.ctx.database.accounts().find_user(account.id).await?.ok_or(ServiceError::NotFound)?.email.to_string())
        };
        let mut result = Contributions::default();
        let mut seen = HashSet::new();
        for repo in repos {
            let reader = self.ctx.repositories.reader(repo.repo.owner_id, repo.repo.id);
            // Fresh repositories and repositories without their configured default branch have no contributions.
            let refs = reader.branch_tips().await?;
            let Some(tip) = refs.iter().find(|r| r.name == repo.repo.default_branch) else { continue };
            let history = self.history(&reader, tip.target.clone()).await?;
            result.partial |= history.1;
            let mut contributed = false;
            for (id, seconds, author) in &history.0 {
                if !(start..end).contains(seconds) {
                    continue;
                }
                if email.as_ref().is_some_and(|email| !author.eq_ignore_ascii_case(email)) {
                    continue;
                }
                contributed = true;
                if seen.insert(id.clone()) {
                    *result.days.entry(seconds.div_euclid(86400)).or_default() += 1;
                }
            }
            result.repositories += usize::from(contributed);
        }
        Ok(result)
    }

    pub async fn repository(&self, repo: &RepoContext, start: i64, end: i64) -> Result<Contributions, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::Forbidden);
        }
        let mut result = Contributions::default();
        let refs = repo.reader.branch_tips().await?;
        if let Some(tip) = refs.iter().find(|r| r.name == repo.repo.repo.default_branch) {
            let history = self.history(&repo.reader, tip.target.clone()).await?;
            result.partial = history.1;
            for (_, seconds, _) in &history.0 {
                if !(start..end).contains(seconds) {
                    continue;
                }
                *result.days.entry(seconds.div_euclid(86400)).or_default() += 1;
            }
            result.repositories = usize::from(!result.days.is_empty());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::Config;
    use rbit_git::{RepositoryReader, RepositoryStorage};
    use rbit_storage::{Database, KvStore};

    #[tokio::test]
    async fn cancelled_scan_populates_cache_and_waiters_reuse_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(&config.storage.data_dir).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let ctx = ServiceContext {
            repositories: RepositoryStorage::new(config.storage.repositories_dir()),
            config: Arc::new(config),
            database,
            kv,
            events: crate::EventBus::default(),
        };
        let service = ContributionService::new(ctx.clone());
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--allow-empty", "-qm", "test"],
        ] {
            assert!(std::process::Command::new("git").current_dir(&repo).args(args).output().unwrap().status.success());
        }
        let output = std::process::Command::new("git").current_dir(&repo).args(["rev-parse", "HEAD"]).output().unwrap();
        let tip = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        let permit = service.workers.acquire_many(2).await.unwrap();
        let first_service = service.clone();
        let first_tip = tip.clone();
        let first = tokio::spawn(async move { first_service.history(&RepositoryReader::new(repo), first_tip).await });
        // Wait until the detached worker owns its shard but is blocked on admission.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if service.scans.iter().any(|scan| scan.try_lock().is_err()) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        first.abort();
        let waiter_service = service.clone();
        let waiter_tip = tip.clone();
        let missing = dir.path().join("does-not-exist");
        let waiter = tokio::spawn(async move { waiter_service.history(&RepositoryReader::new(missing), waiter_tip).await });
        drop(permit);
        let records = waiter.await.unwrap().unwrap();
        assert_eq!(records.0.len(), 1, "waiter must reuse the original scan even though its own reader cannot open Git");
        assert!(!records.1);
        let restarted = ContributionService::new(ctx);
        let _busy = restarted.workers.acquire_many(2).await.unwrap();
        let records = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            restarted.history(&RepositoryReader::new(dir.path().join("missing")), tip),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(records.0.len(), 1, "persistent warm hits bypass scan admission after a service restart");
    }
}
