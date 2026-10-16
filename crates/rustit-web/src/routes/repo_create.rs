//! Repository creation: `GET/POST /new`.
//!
//! The owner picker offers the viewer and every organisation they belong to; `?owner=`
//! preselects one (the organisation page links here with it).

use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rustit_core::{User, Visibility};
use rustit_services::{CreateRepository, ServiceError};
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::SignedIn,
    views::{
        components::{Avatar, FormError},
        home::{NewRepositoryPage, OwnerOption},
        layout::Page,
    },
};

#[derive(Debug, Deserialize)]
pub struct NewRepoQuery {
    owner: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewRepoForm {
    #[serde(default)]
    owner: String,
    name: String,
    #[serde(default)]
    description: String,
    visibility: Option<String>,
}

/// The viewer first, then their organisations; `selected` (case-insensitive) is marked,
/// falling back to the viewer.
pub(super) async fn owner_options(state: &AppState, user: &User, selected: &str) -> WebResult<Vec<OwnerOption>> {
    let memberships = state.services.orgs.memberships(user).await?;
    let mut owners = vec![OwnerOption { name: user.name().to_string(), avatar: Avatar::for_account(&user.account), selected: false }];
    owners.extend(memberships.iter().map(|m| OwnerOption {
        name: m.org.name.to_string(),
        avatar: Avatar::for_account(&m.org),
        selected: false,
    }));
    let index = owners.iter().position(|owner| owner.name.eq_ignore_ascii_case(selected)).unwrap_or(0);
    if let Some(owner) = owners.get_mut(index) {
        owner.selected = true;
    }
    Ok(owners)
}

pub async fn new_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(query): Query<NewRepoQuery>,
) -> WebResult<NewRepositoryPage> {
    let owners = owner_options(&state, &user, query.owner.as_deref().unwrap_or_default()).await?;
    Ok(NewRepositoryPage {
        page: page.titled("New repository"),
        error: FormError::default(),
        owners,
        name: String::new(),
        description: String::new(),
        private: !state.services.instance.current().default_visibility.is_public(),
    })
}

pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<NewRepoForm>,
) -> WebResult<Response> {
    let visibility =
        form.visibility.as_deref().and_then(Visibility::parse).unwrap_or_else(|| state.services.instance.current().default_visibility);
    let input = CreateRepository { name: form.name.clone(), description: form.description.clone(), visibility };

    match state.services.repositories.create_in(&user, &form.owner, input).await {
        Ok(created) => Ok(Redirect::to(&format!("/{}/{}", created.owner.name, created.repo.name)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let page = NewRepositoryPage {
                page: page.titled("New repository"),
                error: FormError::from_message(&error.to_string()),
                owners: owner_options(&state, &user, &form.owner).await?,
                name: form.name,
                description: form.description,
                private: !visibility.is_public(),
            };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}
