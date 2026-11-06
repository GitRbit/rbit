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

use rbit_core::{AccountId, Issue, IssueKind, IssueNumber, Timestamp, references::scan_markdown};

use crate::{RepoContext, ServiceError, ThreadServices};
use rbit_storage::Database;
mod ports;

pub use rbit_collaboration::references::{ReferenceSet, ReferencedIssue};

#[derive(Debug, Clone)]
pub struct ReferenceService {
    database: Database,
    timeline: ThreadServices,
}

impl ReferenceService {
    pub(crate) fn new(database: Database, timeline: ThreadServices) -> Self {
        Self { database, timeline }
    }

    /// Bounded mention choices after repository access checks. Accounts expose only names.
    pub async fn suggestions(&self, repo: &RepoContext, prefix: &str, accounts: bool) -> Result<Vec<(String, String)>, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let prefix: String = prefix.chars().take(100).collect();
        if accounts {
            return Ok(self
                .database
                .issue_meta()
                .mention_candidates(&prefix)
                .await?
                .into_iter()
                .map(|a| (format!("@{}", a.name), a.label().to_owned()))
                .collect());
        }
        let store = self.database.issues();
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
            self.database.issue_search().search(&filter, 10, 0).await?
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
        rbit_collaboration::references::ReferenceResolver::new(&self.view()).resolve(repo.repo.repo.id, texts, scan_markdown).await
    }

    /// Records `CrossReferenced { source }` on every other thread `text` (written in
    /// `source` by `actor`) mentions, once per (target, source). Targets in other
    /// repositories count only when `actor` can read them; without an actor (system
    /// changes) only same-repository targets do.
    pub(crate) async fn record_cross_references(&self, source: &Issue, actor: Option<AccountId>, text: &str) -> Result<(), ServiceError> {
        rbit_collaboration::references::CrossReferences::new(&self.view()).record(source, actor, text, scan_markdown, Timestamp::now).await
    }
    pub(crate) fn effects(
        &self,
    ) -> impl rbit_collaboration::references::CommitReferencePort<Error = ServiceError>
    + rbit_collaboration::references::MergeReferencePort<Error = ServiceError> {
        self.view()
    }
    fn view(&self) -> ports::ReferenceView<'_> {
        ports::ReferenceView { database: &self.database, timeline: &self.timeline }
    }
}
