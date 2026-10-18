//! Visibility-filtered contribution counts derived from default-branch Git history.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use rbit_core::{Account, User};

use crate::{RepoContext, ServiceContext, ServiceError};

type Records = (Vec<(String, i64, String)>, bool);
type HistoryKey = (String, i64, i64);

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
    cache: Arc<tokio::sync::Mutex<HashMap<HistoryKey, Arc<Records>>>>,
}

impl ContributionService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        Self { ctx, cache: Arc::default() }
    }

    /// Immutable tip-keyed data is shared only after the caller filters repositories
    /// by visibility. Bound both entry count and retained records; a push changes the key.
    async fn history(&self, reader: &rbit_git::RepositoryReader, tip: String, start: i64, end: i64) -> Result<Arc<Records>, ServiceError> {
        let key = (tip.clone(), start, end);
        if let Some(records) = self.cache.lock().await.get(&key).cloned() {
            return Ok(records);
        }
        let records = Arc::new(reader.contributions(tip, start, end).await?);
        let mut cache = self.cache.lock().await;
        if cache.len() >= 32 || cache.values().map(|r| r.0.len()).sum::<usize>() + records.0.len() > 250_000 {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&records));
        Ok(records)
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
            let history = self.history(&reader, tip.target.clone(), start, end).await?;
            result.partial |= history.1;
            let mut contributed = false;
            for (id, seconds, author) in &history.0 {
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
            let history = self.history(&repo.reader, tip.target.clone(), start, end).await?;
            result.partial = history.1;
            for (_, seconds, _) in &history.0 {
                *result.days.entry(seconds.div_euclid(86400)).or_default() += 1;
            }
            result.repositories = usize::from(!result.days.is_empty());
        }
        Ok(result)
    }
}
