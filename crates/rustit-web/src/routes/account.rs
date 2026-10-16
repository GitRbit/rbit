//! `/{owner}` profile pages. Users and organisations share the namespace; organisations
//! are rendered by [`super::orgs::profile`].

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
};

use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        account::ProfilePage,
        components::Avatar,
        layout::Page,
    },
};

/// `?tab=stars` switches the profile from owned repositories to starred ones.
#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    tab: Option<String>,
}

pub async fn profile(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path(owner): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> WebResult<Response> {
    let account = state.services.accounts.find_account(&owner).await?;
    if account.is_organization() {
        return super::orgs::profile(&state, &viewer, page, account.name.as_str()).await;
    }
    let own_repos = state.services.repositories.list_for_owner(viewer.user(), &account).await?;
    let starred = state.services.stars.starred_by(viewer.user(), &account).await?;
    let extras = state.services.profiles.public_profile(account.id).await?;

    let tab = if query.tab.as_deref() == Some("stars") { "stars" } else { "repositories" };
    let listed = if tab == "stars" { &starred } else { &own_repos };

    Ok(ProfilePage {
        page: page.titled(account.label().to_owned()),
        name: account.name.to_string(),
        label: account.label().to_owned(),
        bio: account.bio.clone(),
        avatar: Avatar::for_account(&account),
        joined: account.created_at.date(),
        website: extras.website,
        location: extras.location,
        tab,
        repo_count: own_repos.len(),
        star_count: starred.len(),
        repos: super::listing::repo_cards(&state, listed).await?,
        is_self: viewer.user().is_some_and(|u| u.id() == account.id),
    }
    .into_response())
}
