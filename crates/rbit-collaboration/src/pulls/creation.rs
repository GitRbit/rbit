//! Opening a pull request: the checks, their order and their refusal text. Git reads,
//! the network lookup and the capped insert stay behind [`PullCreationPort`].

use std::future::Future;

use super::branch_ref;
use crate::threads::{IssueNumber, IssueText, IssueTextError, IssueTitle};

/// Access already resolved for the actor on both repositories.
#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_excessive_bools, reason = "independent access facts")]
pub struct PullCreationAccess {
    pub same_repo: bool,
    pub base_readable: bool,
    pub head_readable: bool,
    pub base_archived: bool,
}

/// The side of a pull request a branch belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullSide {
    Base,
    Head,
}

/// What the request asks for.
#[derive(Debug, Clone, Copy)]
pub struct PullInput<'a> {
    pub base_branch: &'a str,
    pub head_branch: &'a str,
    /// Empty: the single commit's summary, or the branch name.
    pub title: &'a str,
    pub body: &'a str,
    pub draft: bool,
}

/// How the head compares with the base, counted without file changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchComparison {
    /// A common ancestor exists.
    pub related: bool,
    pub ahead: usize,
    /// The summary of the newest commit ahead.
    pub newest_summary: Option<String>,
}

/// A validated pull request, ready for the capped insert.
#[derive(Debug, Clone)]
pub struct PullDraft {
    pub title: IssueTitle,
    pub body: IssueText,
    pub base_branch: String,
    pub head_branch: String,
    pub head_sha: String,
    pub is_draft: bool,
}

/// Reads and the insert. `Created` is what the store returns for a new pull request.
pub trait PullCreationPort: Sync {
    type Error;
    type Created;

    /// Whether the head repository is in the base's fork network as the actor sees it.
    fn head_in_network(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn base_is_mirror(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// The commit `reference` points at on `side`; `None` when it cannot be read.
    fn tip(&self, side: PullSide, reference: String) -> impl Future<Output = Option<String>> + Send;
    fn compare(&self, base_tip: &str, head_tip: &str) -> impl Future<Output = Result<BranchComparison, Self::Error>> + Send;
    /// Inserts the pull request; `Err(number)` when open pull request `number` already
    /// proposes the same branches (nothing written).
    fn create(&self, draft: PullDraft) -> impl Future<Output = Result<Result<Self::Created, IssueNumber>, Self::Error>> + Send;
}

/// Refusals keep the legacy service's text; persistence and Git errors pass through.
#[derive(Debug)]
pub enum PullCreationError<E> {
    Port(E),
    Text(IssueTextError),
    Invalid(String),
    NotFound,
    Archived,
}

/// Pull request opening over a port.
#[derive(Debug)]
pub struct PullCreation<'a, P> {
    port: &'a P,
    access: PullCreationAccess,
}

impl<'a, P: PullCreationPort> PullCreation<'a, P> {
    pub fn new(port: &'a P, access: PullCreationAccess) -> Self {
        Self { port, access }
    }

    /// Checks `input` and inserts the pull request, spending the actor's budget just
    /// before the insert. Hidden repositories, or a head outside the base's network, are
    /// `NotFound`; an archived base is `Archived`.
    pub async fn create(
        &self,
        input: PullInput<'_>,
        spend: impl FnOnce() -> Result<(), P::Error>,
    ) -> Result<P::Created, PullCreationError<P::Error>> {
        let invalid = |message: &str| PullCreationError::Invalid(message.to_owned());
        if !self.access.base_readable || !self.access.head_readable {
            return Err(PullCreationError::NotFound);
        }
        if !self.access.same_repo && !self.port.head_in_network().await.map_err(PullCreationError::Port)? {
            return Err(PullCreationError::NotFound);
        }
        if self.access.base_archived {
            return Err(PullCreationError::Archived);
        }
        if self.port.base_is_mirror().await.map_err(PullCreationError::Port)? {
            return Err(invalid("pull requests cannot be opened against a mirror"));
        }
        let base_branch = input.base_branch.trim();
        let head_branch = input.head_branch.trim();
        let (Some(base_ref), Some(head_ref)) = (branch_ref(base_branch), branch_ref(head_branch)) else {
            return Err(invalid("choose existing branches to compare"));
        };
        if self.access.same_repo && base_branch == head_branch {
            return Err(invalid("the base and the head branch must differ"));
        }
        let missing = |side: &str| PullCreationError::Invalid(format!("the {side} branch does not exist"));
        let base_tip = self.port.tip(PullSide::Base, base_ref).await.ok_or_else(|| missing("base"))?;
        let head_tip = self.port.tip(PullSide::Head, head_ref).await.ok_or_else(|| missing("head"))?;
        let comparison = self.port.compare(&base_tip, &head_tip).await.map_err(PullCreationError::Port)?;
        if !comparison.related {
            return Err(invalid("the branches have entirely different histories"));
        }
        if comparison.ahead == 0 {
            return Err(PullCreationError::Invalid(format!("{base_branch} already contains every commit of {head_branch}")));
        }
        let title = match input.title.trim() {
            "" if comparison.ahead == 1 => comparison.newest_summary.unwrap_or_else(|| head_branch.to_owned()),
            "" => head_branch.to_owned(),
            title => title.to_owned(),
        };
        let title = IssueTitle::parse(&title).map_err(PullCreationError::Text)?;
        let body = IssueText::body(input.body).map_err(PullCreationError::Text)?;
        spend().map_err(PullCreationError::Port)?;
        let draft = PullDraft {
            title,
            body,
            base_branch: base_branch.to_owned(),
            head_branch: head_branch.to_owned(),
            head_sha: head_tip,
            is_draft: input.draft,
        };
        match self.port.create(draft).await.map_err(PullCreationError::Port)? {
            Ok(created) => Ok(created),
            Err(number) => Err(PullCreationError::Invalid(format!("pull request #{} already proposes these branches", number.get()))),
        }
    }
}
