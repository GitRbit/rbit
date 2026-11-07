//! Ordered qualifier resolution: author before assignee, with legacy English hints.
use super::listing::IssueListing;
use super::{
    IssueReadError, IssueReadPort, SearchFilter,
    query::{IssueQuery, KindFilter, StateFilter, Who},
};
use crate::{IssueState, threads::IssueKind};
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::User;
impl<P: IssueReadPort> IssueListing<'_, P> {
    pub(super) async fn search_filter(
        &self,
        viewer: Option<&User>,
        repo: RepoId,
        query: &IssueQuery,
    ) -> Result<(SearchFilter, Vec<String>), IssueReadError<P::Error>> {
        let mut hints: Vec<String> = query.ignored.iter().map(|term| format!("“{term}” is not understood and was ignored")).collect();
        let mut filter = SearchFilter::new(repo);
        filter.kind = query.kind.map(|kind| match kind {
            KindFilter::Issue => IssueKind::Issue,
            KindFilter::Pull => IssueKind::Pull,
        });
        filter.state = query.state.map(|state| match state {
            StateFilter::Open => IssueState::Open,
            StateFilter::Closed => IssueState::Closed,
        });
        filter.merged = query.merged;
        filter.draft = query.draft;
        filter.locked = query.locked;
        filter.author = self.resolve_who(viewer, query.author.as_ref(), &mut hints).await?;
        filter.assignee = self.resolve_who(viewer, query.assignee.as_ref(), &mut hints).await?;
        filter.no_assignee = query.no_assignee;
        filter.labels.clone_from(&query.labels);
        filter.not_labels.clone_from(&query.not_labels);
        filter.no_label = query.no_label;
        filter.milestone.clone_from(&query.milestone);
        filter.no_milestone = query.no_milestone;
        filter.text.clone_from(&query.text);
        filter.sort = query.sort;

        Ok((filter, hints))
    }

    /// The account id `who` names: unknown names match nothing (`AccountId(0)`), `@me`
    /// without a viewer is ignored with a hint.
    async fn resolve_who(
        &self,
        viewer: Option<&User>,
        who: Option<&Who>,
        hints: &mut Vec<String>,
    ) -> Result<Option<AccountId>, IssueReadError<P::Error>> {
        Ok(match who {
            None => None,
            Some(Who::Me) => {
                if viewer.is_none() {
                    hints.push("Sign in to use @me".to_owned());
                }
                viewer.map(User::id)
            }
            Some(Who::Name(name)) => {
                Some(self.port.account_named(name).await.map_err(IssueReadError::Port)?.map_or(AccountId(0), |account| account.id))
            }
        })
    }
}
