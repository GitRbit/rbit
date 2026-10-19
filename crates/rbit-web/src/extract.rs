//! Request extractors.
//!
//! The session middleware resolves the cookie once per request and stores a [`Viewer`]
//! in the request extensions; these extractors only read it. (They are `async` only
//! because the `FromRequestParts` trait requires it.)
use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use rbit_auth::SessionToken;
use rbit_core::User;

use crate::{AppState, error::WebError, themes::Theme, urls::encode_path, views::layout::Page};

/// The user making the request, if signed in.
#[derive(Debug, Clone, Default)]
pub struct Viewer(pub Option<Arc<User>>);

impl Viewer {
    pub fn user(&self) -> Option<&User> {
        self.0.as_deref()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Viewer {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Self>().cloned().unwrap_or_default())
    }
}

/// The session token of a signed-in request (set by the session middleware), so pages
/// can mark "this device" and keep it when signing out everywhere else.
#[derive(Debug, Clone)]
pub struct CurrentSession(pub SessionToken);

impl<S: Send + Sync> FromRequestParts<S> for CurrentSession {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Self>().cloned().ok_or_else(|| Redirect::to("/login").into_response())
    }
}

/// A signed-in user. Anonymous requests are redirected to the sign-in page and returned
/// to where they were afterwards.
#[derive(Debug, Clone)]
pub struct SignedIn(pub Arc<User>);

impl<S: Send + Sync> FromRequestParts<S> for SignedIn {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<Viewer>().and_then(|v| v.0.clone()) {
            return Ok(Self(user));
        }
        let next = parts.uri.path_and_query().map_or("/", |pq| pq.as_str());
        Err(Redirect::to(&format!("/login?next={}", encode_path(next).replace('&', "%26"))).into_response())
    }
}

/// A signed-in site administrator. Anonymous requests are sent to sign in; everyone else
/// gets the ordinary 404 page, so the administration panel's existence isn't revealed.
#[derive(Debug, Clone)]
pub struct SiteAdmin(pub Arc<User>);

impl<S: Send + Sync> FromRequestParts<S> for SiteAdmin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let SignedIn(user) = SignedIn::from_request_parts(parts, state).await?;
        if user.is_admin { Ok(Self(user)) } else { Err(WebError::NotFound.into_response()) }
    }
}

impl FromRequestParts<AppState> for Page {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let viewer = Viewer::from_request_parts(parts, state).await?;
        let mut page = Page::new(state, viewer.user(), parts.uri.path()).with_theme(Theme::from_headers(&parts.headers));
        if let Some(user) = viewer.user().filter(|_| state.config.issues.notifications) {
            page.has_unread_notifications = state.services.notifications.has_unread(user.id()).await.unwrap_or_else(|error| {
                tracing::warn!(%error, "cannot check for unread notifications");
                false
            });
        }
        Ok(page)
    }
}
