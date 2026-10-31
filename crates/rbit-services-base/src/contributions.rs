//! Visibility-filtered contribution counts derived from default-branch Git history.
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    sync::Arc,
};

use rbit_core::{Account, User};

use crate::{RepoContext, ServiceContext, ServiceError};

type Records = rbit_storage::kv::CachedContributions;
type DailyRecords = VecDeque<(String, Arc<Contributions>)>;

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
    daily: Arc<tokio::sync::Mutex<DailyRecords>>,
    scans: Arc<[tokio::sync::Mutex<()>; 16]>,
    workers: Arc<tokio::sync::Semaphore>,
}

impl ContributionService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self {
            cache: ctx.kv.insights(),
            hot: Arc::default(),
            daily: Arc::default(),
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
        // A user's commits are those authored with their noreply address, which is public.
        // Their account email is private: matching it for anyone else would let a viewer
        // confirm a guessed address by committing with it to a repository they can read,
        // so it counts only when the user (or a site administrator) is looking.
        let emails = if account.is_organization() {
            None
        } else {
            let user = self.ctx.database.accounts().find_user(account.id).await?.ok_or(ServiceError::NotFound)?;
            let config = &self.ctx.config;
            let domain = rbit_core::pull::noreply_domain(&config.pulls.noreply_domain, &config.server.public_url);
            let mut emails = vec![rbit_core::pull::noreply_email(user.name().as_str(), &domain)];
            if viewer.is_some_and(|viewer| viewer.id() == account.id || viewer.is_admin) {
                emails.push(user.email.to_string());
            }
            Some(emails)
        };
        let mut result = Contributions::default();
        let mut seen = HashSet::new();
        for repo in repos {
            let reader = self.ctx.repositories.reader(repo.repo.owner_id, repo.repo.id);
            // Fresh repositories and repositories without their configured default branch
            // have no contributions. Only that one branch is resolved, not every branch.
            let tip = match reader.commit_id(format!("refs/heads/{}", repo.repo.default_branch)).await {
                Ok(tip) => tip,
                Err(rbit_git::GitError::NotFound(_)) => continue,
                Err(error) => {
                    // An unreadable branch never hid the rest of the profile; keep it that way.
                    tracing::warn!(%error, repo = repo.repo.id.0, "default branch unreadable for contributions");
                    continue;
                }
            };
            let history = self.history(&reader, tip).await?;
            result.partial |= history.1;
            let mut contributed = false;
            for (id, seconds, author) in &history.0 {
                if !(start..end).contains(seconds) {
                    continue;
                }
                if emails.as_ref().is_some_and(|emails| !emails.iter().any(|email| author.eq_ignore_ascii_case(email))) {
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
        let tip = match repo.reader.commit_id(format!("refs/heads/{}", repo.repo.repo.default_branch)).await {
            Ok(tip) => tip,
            Err(rbit_git::GitError::NotFound(_)) => return Ok(Contributions::default()),
            Err(error) => return Err(error.into()),
        };
        // Public callers can request partial days; keep their exact second bounds.
        if start.rem_euclid(86400) != 0 || end.rem_euclid(86400) != 0 {
            let history = self.history(&repo.reader, tip).await?;
            return Ok(count_days(&history, start, end));
        }
        let cached = {
            let mut cache = self.daily.lock().await;
            cache.iter().position(|(id, _)| id == &tip).and_then(|index| cache.remove(index)).map(|entry| {
                let result = Arc::clone(&entry.1);
                cache.push_back(entry);
                result
            })
        };
        let daily = if let Some(daily) = cached {
            daily
        } else {
            let history = self.history(&repo.reader, tip.clone()).await?;
            let daily = Arc::new(
                tokio::task::spawn_blocking(move || count_days(&history, i64::MIN, i64::MAX)).await.map_err(ServiceError::internal)?,
            );
            let mut cache = self.daily.lock().await;
            cache.retain(|(id, _)| id != &tip);
            while cache.len() >= 32 || cache.iter().map(|(_, entry)| entry.days.len()).sum::<usize>() + daily.days.len() > 25_000 {
                if cache.pop_front().is_none() {
                    break;
                }
            }
            if daily.days.len() <= 25_000 {
                cache.push_back((tip, Arc::clone(&daily)));
            }
            daily
        };
        let days: BTreeMap<_, _> = if start < end {
            daily.days.range(start.div_euclid(86400)..end.div_euclid(86400)).map(|(&day, &count)| (day, count)).collect()
        } else {
            BTreeMap::new()
        };
        Ok(Contributions { repositories: usize::from(!days.is_empty()), days, partial: daily.partial })
    }
}

fn count_days(history: &Records, start: i64, end: i64) -> Contributions {
    let mut result = Contributions { partial: history.1, ..Contributions::default() };
    for (_, seconds, _) in &history.0 {
        if (start..end).contains(seconds) {
            *result.days.entry(seconds.div_euclid(86400)).or_default() += 1;
        }
    }
    result.repositories = usize::from(!result.days.is_empty());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::Config;
    use rbit_git::{RepositoryReader, RepositoryStorage};
    use rbit_storage::{Database, KvStore};

    #[test]
    fn daily_counts_keep_second_bounds_negative_dates_and_partial_status() {
        let records = (vec![("a".into(), -1, "a@b".into()), ("b".into(), 0, "a@b".into()), ("c".into(), 86400, "a@b".into())], true);
        let all = count_days(&records, i64::MIN, i64::MAX);
        assert_eq!(all.days, BTreeMap::from([(-1, 1), (0, 1), (1, 1)]));
        assert!(all.partial);
        assert_eq!(count_days(&records, -1, 86400).days, BTreeMap::from([(-1, 1), (0, 1)]));
        assert_eq!(count_days(&records, 1, 86400).repositories, 0);
    }

    async fn calendar_context(ctx: &ServiceContext, path: std::path::PathBuf, tip: &str) -> RepoContext {
        let owner = ctx
            .database
            .accounts()
            .create_user(rbit_storage::NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.test".parse().unwrap(),
                display_name: None,
                password_hash: "unused".into(),
                is_admin: false,
            })
            .await
            .unwrap();
        let repository = ctx
            .database
            .repositories()
            .create(rbit_storage::NewRepository {
                owner_id: owner.id(),
                name: "activity".parse().unwrap(),
                description: None,
                visibility: rbit_core::Visibility::Public,
                default_branch: "main".into(),
            })
            .await
            .unwrap();
        // Point the fixture's default branch at the commit we scanned above.
        assert!(
            std::process::Command::new("git").current_dir(&path).args(["branch", "-f", "calendar", tip]).output().unwrap().status.success()
        );
        let mut repository = repository;
        repository.default_branch = "calendar".into();
        RepoContext {
            reader: RepositoryReader::new(path.clone()),
            disk_path: path,
            repo: rbit_core::OwnedRepository { owner: owner.account, repo: repository },
            access: rbit_core::RepoAccess::Read,
            viewer: None,
            collaborator: None,
        }
    }

    #[tokio::test]
    async fn account_calendars_read_only_the_default_branch() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = crate::Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let signup = crate::SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
        let ada = services.accounts.create_user(signup, Some(true)).await.unwrap();
        for name in ["work", "empty"] {
            let input =
                crate::CreateRepository { name: name.into(), description: String::new(), visibility: rbit_core::Visibility::Public };
            services.repositories.create(&ada, input).await.unwrap();
        }
        let bare = services.repositories.open(Some(&ada), "ada", "work").await.unwrap().disk_path;

        let work = dir.path().join("clone");
        std::fs::create_dir(&work).unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .current_dir(&work)
                .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["commit", "-q", "--allow-empty", "-m", "on main"]);
        git(&["checkout", "-q", "-b", "side"]);
        git(&["commit", "-q", "--allow-empty", "-m", "on side"]);
        git(&["commit", "-q", "--allow-empty", "-m", "more on side"]);
        git(&["push", "-q", bare.to_str().unwrap(), "main", "side"]);

        let counts = services.contributions.account(Some(&ada), &ada.account, 0, i64::MAX).await.unwrap();
        assert_eq!(counts.days.values().sum::<usize>(), 1, "side-branch commits are not contributions");
        assert_eq!(counts.repositories, 1, "the empty repository has no default branch yet");
    }

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
            actions_availability: crate::ActionsAvailability::default(),
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
        let repo = calendar_context(&ctx, dir.path().join("repo"), &tip).await;
        let counts = service.repository(&repo, 0, 253_402_300_800).await.unwrap();
        assert_eq!(counts.days.values().sum::<usize>(), 1);
        service.hot.lock().await.clear();
        let occupied = service.workers.acquire_many(2).await.unwrap();
        let warm =
            tokio::time::timeout(std::time::Duration::from_secs(1), service.repository(&repo, 0, 253_402_300_800)).await.unwrap().unwrap();
        assert_eq!(warm.days, counts.days, "daily totals must not need history scan admission");
        let empty = service.repository(&repo, 0, 86400).await.unwrap();
        assert!(empty.days.is_empty(), "another year reads the daily cache too");
        drop(occupied);
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
