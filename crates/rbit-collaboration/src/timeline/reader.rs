//! Selected-page batching and permission filtering without extra row/DTO buffers.
use super::{
    CrossReferenceSource, EventSubjects, MAX_TIMELINE_PAGE, StoredTimelineItem, Timeline, TimelineCursor, TimelineEntry, TimelineReadPort,
};
use crate::{
    comments::{Comment, CommentId},
    threads::IssueId,
};
use rbit_contracts::AccountId;
use rbit_identity_models::Account;
use std::collections::{HashMap, HashSet};
#[derive(Debug)]
pub struct TimelineReader<'a, P> {
    port: &'a P,
}
impl<'a, P: TimelineReadPort> TimelineReader<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn find_comment(&self, issue: IssueId, id: CommentId) -> Result<Option<(Comment, Option<Account>)>, P::Error> {
        let Some(comment) = self.port.comment(issue, id).await? else {
            return Ok(None);
        };
        let author = match comment.author {
            Some(author) => self.port.accounts(&[author]).await?.into_iter().next(),
            None => None,
        };
        Ok(Some((comment, author)))
    }
    pub async fn read(&self, issue: IssueId, cursor: Option<TimelineCursor>, limit: u32) -> Result<Timeline<P::Event>, P::Error> {
        let page = self.port.page(issue, cursor, limit.clamp(1, MAX_TIMELINE_PAGE)).await?;
        let mut account_ids = HashSet::new();
        let mut source_ids = HashSet::new();
        for item in &page.items {
            if let StoredTimelineItem::Event { event, .. } = item {
                account_ids.extend(P::subject_account(event));
                if let Some(source) = P::cross_reference(event) {
                    source_ids.insert(source);
                }
            }
        }
        let account_ids: Vec<AccountId> = account_ids.into_iter().collect();
        let source_ids: Vec<IssueId> = source_ids.into_iter().collect();
        let (accounts, sources) = self.port.subjects(&account_ids, &source_ids).await?;
        let accounts: HashMap<AccountId, Account> = accounts.into_iter().map(|account| (account.id, account)).collect();
        let sources: HashMap<IssueId, CrossReferenceSource> = sources
            .into_iter()
            .map(|source| {
                let issue = source.issue;
                let reference = CrossReferenceSource {
                    full_name: source.full_name,
                    number: issue.number,
                    kind: issue.kind,
                    state: issue.state,
                    title: issue.title,
                };
                (issue.id, reference)
            })
            .collect();
        let permissions = self.port.permissions();
        let entries = page
            .items
            .into_iter()
            .filter_map(|item| match item {
                StoredTimelineItem::Comment { comment, author } => {
                    let allowed = self.port.comment_access(permissions, &comment);
                    Some(TimelineEntry::Comment { comment, author, can_edit: allowed.can_edit, can_delete: allowed.can_delete })
                }
                StoredTimelineItem::Event { id, actor, event, at, .. } => {
                    let source = match P::cross_reference(&event) {
                        Some(source) => Some(sources.get(&source).cloned()?),
                        None => None,
                    };
                    let account = P::subject_account(&event).and_then(|id| accounts.get(&id).cloned());
                    Some(TimelineEntry::Event { id, actor, event, at, subjects: EventSubjects { account, source } })
                }
            })
            .collect();
        Ok(Timeline { entries, next: page.next })
    }
}
