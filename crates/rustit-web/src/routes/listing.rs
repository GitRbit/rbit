//! Helpers shared by pages that list repositories.

use rustit_core::repository::OwnedRepository;

use crate::{AppState, error::WebResult, views::components::RepoCard};

/// Repository cards with star counts, fetched in one batched query for the whole list.
pub(super) async fn repo_cards(state: &AppState, repos: &[OwnedRepository]) -> WebResult<Vec<RepoCard>> {
    let star_counts = state.services.stars.counts(repos).await?;
    Ok(RepoCard::list(repos, &star_counts))
}
