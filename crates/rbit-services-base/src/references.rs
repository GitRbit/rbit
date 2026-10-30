//! Resolving references (`#12`, `owner/repo#12`, `@name`) found in Markdown, so pages
//! can link the ones that exist, and recording cross-references between threads.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! [`ReferenceService::resolve`] is a hook (`docs/design/milestone-2b.md` §3): T0's
//! timeline views and P call it before rendering with
//! `rbit_web::render::references::render_issue_markdown`.
//!
//! Privacy: `#N` resolves only inside the repository being viewed (already readable);
//! `owner/repo#N` is never looked up for rendering; `@name` resolves for any existing
//! account, whatever it may read, so a mention never reveals access. Cross-references
//! are recorded only on targets the author of the mention can read, and T0 shows them
//! only to viewers who can read the source.

use std::collections::BTreeMap;

use rbit_auth::repo_access;
use rbit_core::{
    AccountId, Issue, IssueKind, IssueNumber, IssueState, TimelineEvent, Timestamp,
    references::{MAX_MENTIONS, scan_markdown},
};

use crate::{RepoContext, ServiceContext, ServiceError, TimelineService, repositories::viewer_grants};

/// An existing thread a `#N` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencedIssue {
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// What the references in a batch of texts resolve to: the existing issues of the
/// repository among `#N` and the existing accounts among `@name`. The default resolves
/// nothing (every reference renders as plain text).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    /// Existing threads of the repository, by number.
    pub issues: BTreeMap<IssueNumber, ReferencedIssue>,
    /// Existing accounts: lower-cased name → the name as stored.
    pub accounts: BTreeMap<String, String>,
}

impl ReferenceSet {
    pub fn issue(&self, number: IssueNumber) -> Option<&ReferencedIssue> {
        self.issues.get(&number)
    }

    /// The stored spelling of account `name`, if it exists.
    pub fn account(&self, name: &str) -> Option<&str> {
        self.accounts.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceService {
    ctx: ServiceContext,
    timeline: TimelineService,
}

impl ReferenceService {
    pub(crate) fn new(ctx: ServiceContext, timeline: TimelineService) -> Self {
        Self { ctx, timeline }
    }

    /// Bounded mention choices after repository access checks. Accounts expose only names.
    pub async fn suggestions(&self, repo: &RepoContext, prefix: &str, accounts: bool) -> Result<Vec<(String, String)>, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let prefix: String = prefix.chars().take(100).collect();
        if accounts {
            return Ok(self
                .ctx
                .database
                .issue_meta()
                .mention_candidates(&prefix)
                .await?
                .into_iter()
                .map(|a| (format!("@{}", a.name), a.label().to_owned()))
                .collect());
        }
        let store = self.ctx.database.issues();
        let issues = if let Ok(number) = IssueNumber::parse(&prefix) {
            return Ok(store
                .find_numbers(repo.repo.repo.id, &[number])
                .await?
                .into_iter()
                .map(|issue| {
                    (
                        format!("#{}", issue.number.get()),
                        format!("{} · {}", if issue.kind == IssueKind::Pull { "PR" } else { "Issue" }, issue.title),
                    )
                })
                .collect());
        } else {
            let mut filter = rbit_storage::sqlite::issue_search::SearchFilter::new(repo.repo.repo.id);
            filter.text = prefix.split_whitespace().map(str::to_owned).collect();
            self.ctx.database.issue_search().search(&filter, 10, 0).await?
        };
        Ok(issues
            .into_iter()
            .map(|issue| {
                (format!("#{}", issue.number.get()), format!("{} · {}", if issue.is_pull() { "PR" } else { "Issue" }, issue.title))
            })
            .collect())
    }

    /// Resolves the `#N` and `@name` references among `texts` in `repo` (already
    /// access-checked), with one batch query for each kind.
    pub async fn resolve(&self, repo: &RepoContext, texts: &[&str]) -> Result<ReferenceSet, ServiceError> {
        let mut numbers: Vec<IssueNumber> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for text in texts {
            let scan = scan_markdown(text);
            for number in scan.local {
                if !numbers.contains(&number) {
                    numbers.push(number);
                }
            }
            for name in scan.mentions {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        // Bounded like any one text, scaled by the number of texts.
        names.truncate(MAX_MENTIONS.saturating_mul(texts.len().max(1)));
        let database = &self.ctx.database;
        let issue_store = database.issues();
        let meta_store = database.issue_meta();
        let (issues, accounts) =
            tokio::try_join!(issue_store.find_numbers(repo.repo.repo.id, &numbers), meta_store.accounts_named(&names))?;
        Ok(ReferenceSet {
            issues: issues
                .into_iter()
                .map(|issue| {
                    let referenced = ReferencedIssue { number: issue.number, kind: issue.kind, state: issue.state, title: issue.title };
                    (issue.number, referenced)
                })
                .collect(),
            accounts: accounts.into_iter().map(|account| (account.name.as_str().to_ascii_lowercase(), account.name.to_string())).collect(),
        })
    }

    /// Records `CrossReferenced { source }` on every other thread `text` (written in
    /// `source` by `actor`) mentions, once per (target, source). Targets in other
    /// repositories count only when `actor` can read them; without an actor (system
    /// changes) only same-repository targets do.
    pub(crate) async fn record_cross_references(&self, source: &Issue, actor: Option<AccountId>, text: &str) -> Result<(), ServiceError> {
        let scan = scan_markdown(text);
        if scan.local.is_empty() && scan.remote.is_empty() {
            return Ok(());
        }
        let database = &self.ctx.database;
        let mut targets: Vec<Issue> = Vec::new();
        for target in database.issues().find_numbers(source.repo_id, &scan.local).await? {
            if let Some(issue) = database.issues().find(target.id).await? {
                targets.push(issue);
            }
        }
        let actor_user = match actor {
            Some(id) => database.accounts().find_active_user(id).await?,
            None => None,
        };
        if let Some(user) = &actor_user {
            let repositories = database.repositories();
            for remote in &scan.remote {
                let Some(repo) = repositories.find_by_path(&remote.owner, &remote.repo).await? else { continue };
                if repo.repo.id == source.repo_id {
                    if let Some(issue) = database.issues().find_by_number(repo.repo.id, remote.number).await? {
                        targets.push(issue);
                    }
                    continue;
                }
                let grants = viewer_grants(&repositories, Some(user), &repo).await?;
                if !repo_access(Some(user), &repo, &grants).can_read() {
                    continue;
                }
                if let Some(issue) = database.issues().find_by_number(repo.repo.id, remote.number).await? {
                    targets.push(issue);
                }
            }
        }
        let mut seen = Vec::new();
        for target in targets {
            if target.id == source.id || seen.contains(&target.id) {
                continue;
            }
            seen.push(target.id);
            if database.issue_meta().record_cross_ref(target.id, source.id, Timestamp::now()).await? {
                self.timeline.record(target.id, actor, TimelineEvent::CrossReferenced { source: source.id }).await?;
            }
        }
        Ok(())
    }
}
