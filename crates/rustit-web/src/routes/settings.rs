//! Account settings: `/settings/…`.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rustit_core::{AccessTokenId, TokenScope, User};
use rustit_services::{AccessTokenService, CreateAccessToken, ServiceError, profiles::UpdateProfile};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::SignedIn,
    views::{
        components::FormError,
        layout::Page,
        settings::{
            AccountForm, AccountPage, CreatedToken, ProfileForm, ProfilePage, SettingsSection, TokenForm, TokensPage, parse_expiry,
        },
    },
};

const TOKENS_URL: &str = SettingsSection::AccessTokens.url();

/// Routes for `/settings/…`, merged into the page router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(index))
        .route("/settings/profile", get(profile).post(update_profile))
        .route("/settings/account", get(account))
        .route("/settings/account/email", post(change_email))
        .route("/settings/account/password", post(change_password))
        .route("/settings/tokens", get(tokens).post(create_token))
        .route("/settings/tokens/{id}/revoke", post(revoke_token))
}

/// `GET /settings`: the first settings section.
pub async fn index() -> Redirect {
    Redirect::to(SettingsSection::Profile.url())
}

/// `?saved=…`, set by the redirect after a successful change.
#[derive(Debug, Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

// ------------------------------------------------------------------ profile

#[derive(Debug, Deserialize)]
pub struct ProfileFormInput {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    website: String,
    #[serde(default)]
    location: String,
}

/// `GET /settings/profile`
pub async fn profile(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Query(query): Query<SavedQuery>,
) -> WebResult<ProfilePage> {
    let profile = state.services.profiles.profile(&user).await?;
    Ok(ProfilePage::new(page, &user, profile.into()).saved(query.saved.is_some()))
}

/// `POST /settings/profile`
pub async fn update_profile(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<ProfileFormInput>,
) -> WebResult<Response> {
    let input = UpdateProfile {
        display_name: form.display_name.clone(),
        bio: form.bio.clone(),
        website: form.website.clone(),
        location: form.location.clone(),
    };
    match state.services.profiles.update_profile(&user, input).await {
        Ok(_) => Ok(Redirect::to(&format!("{}?saved=1", SettingsSection::Profile.url())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let submitted = ProfileForm { display_name: form.display_name, bio: form.bio, website: form.website, location: form.location };
            let page = ProfilePage::new(page, &user, submitted).with_error(FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

// ------------------------------------------------------------------ account

#[derive(Debug, Deserialize)]
pub struct EmailFormInput {
    #[serde(default)]
    email: String,
    #[serde(default)]
    current_password: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordFormInput {
    #[serde(default, rename = "current_password")]
    current: String,
    #[serde(default, rename = "new_password")]
    new: String,
    #[serde(default, rename = "confirm_password")]
    confirmation: String,
}

/// `GET /settings/account`
pub async fn account(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page, Query(query): Query<SavedQuery>) -> AccountPage {
    let saved = query.saved.as_deref().and_then(AccountForm::parse);
    AccountPage::new(page, &user, state.config.security.min_password_length).saved(saved)
}

/// `POST /settings/account/email`
pub async fn change_email(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<EmailFormInput>,
) -> WebResult<Response> {
    let result = state.services.profiles.change_email(&user, form.current_password, &form.email).await;
    account_result(&state, &user, page, AccountForm::Email, result.map(drop), Some(form.email))
}

/// `POST /settings/account/password`
pub async fn change_password(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<PasswordFormInput>,
) -> WebResult<Response> {
    let result = if form.new == form.confirmation {
        state.services.profiles.change_password(&user, form.current, form.new).await
    } else {
        Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
    };
    account_result(&state, &user, page, AccountForm::Password, result, None)
}

/// Redirects after a successful change, or re-renders the account page with the error
/// shown above `form`.
fn account_result(
    state: &AppState,
    user: &User,
    page: Page,
    form: AccountForm,
    result: Result<(), ServiceError>,
    submitted_email: Option<String>,
) -> WebResult<Response> {
    let message = match result {
        Ok(()) => return Ok(Redirect::to(&format!("{}?saved={}", SettingsSection::Account.url(), form.key())).into_response()),
        Err(ServiceError::Invalid(message)) => message,
        Err(ServiceError::InvalidCredentials) => "current password is incorrect".to_owned(),
        Err(conflict @ ServiceError::Conflict(_)) => conflict.to_string(),
        Err(other) => return Err(other.into()),
    };
    let page = AccountPage::new(page, user, state.config.security.min_password_length).with_error(
        form,
        FormError::from_message(&message),
        submitted_email,
    );
    Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
}

// ------------------------------------------------------------------ access tokens

#[derive(Debug, Deserialize)]
pub struct CreateTokenForm {
    #[serde(default)]
    name: String,
    scope: Option<String>,
    expires: Option<String>,
}

/// `GET /settings/tokens`
pub async fn tokens(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<TokensPage> {
    let tokens = state.services.tokens.list(&user).await?;
    Ok(TokensPage::new(page, user.name().to_string(), &tokens, TokenForm::blank(AccessTokenService::EXPIRY_CHOICES)))
}

/// `POST /settings/tokens`: creates a token and renders its secret exactly once.
///
/// Deliberately not a redirect: the secret is never stored, so it can only travel in
/// this response, which is marked `no-store`.
pub async fn create_token(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<CreateTokenForm>,
) -> WebResult<Response> {
    let choices = AccessTokenService::EXPIRY_CHOICES;
    let scope = form.scope.as_deref().and_then(TokenScope::parse).unwrap_or_default();
    let expiry = form.expires.as_deref().and_then(|value| parse_expiry(value, choices));
    let username = user.name().to_string();

    let result = match expiry {
        Some(expires_in_days) => {
            let input = CreateAccessToken { name: form.name.trim().to_owned(), scope, expires_in_days };
            state.services.tokens.create(&user, input).await
        }
        None => Err(ServiceError::Invalid("choose when the token expires".to_owned())),
    };

    match result {
        Ok((token, secret)) => {
            let tokens = state.services.tokens.list(&user).await?;
            let page =
                TokensPage::new(page, username, &tokens, TokenForm::blank(choices)).with_created(CreatedToken { name: token.name, secret });
            Ok(([(header::CACHE_CONTROL, "no-store")], page).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            let tokens = state.services.tokens.list(&user).await?;
            let form = TokenForm::submitted(form.name, scope, expiry, choices);
            let page = TokensPage::new(page, username, &tokens, form).with_error(FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /settings/tokens/{id}/revoke`
pub async fn revoke_token(State(state): State<AppState>, SignedIn(user): SignedIn, Path(id): Path<String>) -> WebResult<Redirect> {
    let id = id.parse().map(AccessTokenId).map_err(|_| WebError::NotFound)?;
    state.services.tokens.revoke(&user, id).await?;
    Ok(Redirect::to(TOKENS_URL))
}
