//! Pull mirrors: `/new/mirror` and `/{owner}/{repo}/settings/mirror` (stretch goal).
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. The settings page is for repository
//! administrators of mirrors only; everyone else gets 404.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{User, Visibility};
use rbit_services::{ServiceError, mirrors::CreateMirror};
use serde::Deserialize;

use super::{repo::RepoPage, repo_create::owner_options};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        mirrors::{MirrorSettingsPage, MirrorView, NewMirrorPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/new/mirror", get(new_form).post(create))
        .route("/{owner}/{repo}/settings/mirror", get(settings).post(update))
        .route("/{owner}/{repo}/settings/mirror/sync", post(sync))
        .route("/{owner}/{repo}/settings/mirror/remove", post(remove))
}

// ------------------------------------------------------------------ creating

#[derive(Debug, Deserialize)]
pub struct NewMirrorForm {
    #[serde(default)]
    owner: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    visibility: Option<String>,
    #[serde(default)]
    interval: String,
}

/// The name a mirror of `url` gets by default: the last path segment without `.git`.
fn suggested_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/');
    let last = path.rsplit('/').next().unwrap_or_default();
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

async fn mirror_page(state: &AppState, user: &User, page: Page, form: &NewMirrorForm, error: FormError) -> WebResult<NewMirrorPage> {
    let mirrors = &state.services.mirrors;
    Ok(NewMirrorPage {
        page: page.titled("Mirror a repository"),
        error,
        disabled: !mirrors.is_enabled(),
        owners: owner_options(state, user, &form.owner).await?,
        url: form.url.clone(),
        name: form.name.clone(),
        description: form.description.clone(),
        private: form.visibility.as_deref() == Some(Visibility::Private.as_str()),
        interval_minutes: if form.interval.is_empty() { mirrors.default_interval_minutes().to_string() } else { form.interval.clone() },
        min_interval_minutes: mirrors.min_interval_minutes(),
    })
}

/// `GET /new/mirror`
async fn new_form(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<NewMirrorPage> {
    let form = NewMirrorForm {
        owner: String::new(),
        url: String::new(),
        name: String::new(),
        description: String::new(),
        visibility: Some(state.services.instance.current().default_visibility.as_str().to_owned()),
        interval: String::new(),
    };
    mirror_page(&state, &user, page, &form, FormError::default()).await
}

/// `POST /new/mirror`: redirects to the new (still empty) mirror.
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<NewMirrorForm>,
) -> WebResult<Response> {
    let interval = match form.interval.trim() {
        "" => Ok(None),
        text => text.parse().map(Some).map_err(|_| ServiceError::Invalid("the interval must be a number of minutes".to_owned())),
    };
    let visibility =
        form.visibility.as_deref().and_then(Visibility::parse).unwrap_or_else(|| state.services.instance.current().default_visibility);
    let name = match form.name.trim() {
        "" => suggested_name(form.url.trim()),
        name => name.to_owned(),
    };
    let result = match interval {
        Ok(interval_minutes) => {
            let input = CreateMirror { name, description: form.description.clone(), visibility, url: form.url.clone(), interval_minutes };
            state.services.mirrors.create(&user, &form.owner, input).await
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(created) => Ok(Redirect::to(&format!("/{}/{}", created.owner.name, created.repo.name)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let view = mirror_page(&state, &user, page, &form, FormError::from_message(&error.to_string())).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

// ------------------------------------------------------------------ settings

/// Loads a mirror for one of its administrators; 404 for everyone and everything else.
async fn load_mirror(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<(RepoPage, rbit_core::mirror::Mirror)> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    let mirror = state.services.mirrors.find(&loaded.ctx).await?.ok_or(WebError::NotFound)?;
    Ok((loaded, mirror))
}

fn settings_view(state: &AppState, loaded: &RepoPage, mirror: &rbit_core::mirror::Mirror, page: Page) -> MirrorSettingsPage {
    MirrorSettingsPage {
        page: page.titled(format!("Mirror · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        mirror: MirrorView::from(mirror),
        error: FormError::default(),
        notice: None,
        min_interval_minutes: state.services.mirrors.min_interval_minutes(),
        action_url: format!("{}/settings/mirror", loaded.urls.home()),
    }
}

#[derive(Debug, Deserialize)]
pub struct SettingsQuery {
    saved: Option<String>,
    queued: Option<String>,
}

/// `GET /{owner}/{repo}/settings/mirror`
async fn settings(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<SettingsQuery>,
) -> WebResult<MirrorSettingsPage> {
    let (loaded, mirror) = load_mirror(&state, &viewer, &owner, &repo).await?;
    let mut view = settings_view(&state, &loaded, &mirror, page);
    view.notice = match (query.saved, query.queued) {
        (Some(_), _) => Some("Saved."),
        (None, Some(_)) => Some("A fetch is queued; refresh in a moment to see its outcome."),
        (None, None) => None,
    };
    Ok(view)
}

#[derive(Debug, Deserialize)]
pub struct IntervalForm {
    #[serde(default)]
    interval: String,
}

/// `POST /{owner}/{repo}/settings/mirror`: the fetch interval.
async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<IntervalForm>,
) -> WebResult<Response> {
    let (loaded, mirror) = load_mirror(&state, &viewer, &owner, &repo).await?;
    let result = match form.interval.trim().parse() {
        Ok(minutes) => state.services.mirrors.set_interval(&loaded.ctx, minutes).await,
        Err(_) => Err(ServiceError::Invalid("the interval must be a number of minutes".to_owned())),
    };
    match result {
        Ok(()) => Ok(Redirect::to(&format!("{}/settings/mirror?saved=1", loaded.urls.home())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let mut view = settings_view(&state, &loaded, &mirror, page);
            view.error = FormError::from_message(&message);
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /{owner}/{repo}/settings/mirror/sync`: fetch now (in the background).
async fn sync(State(state): State<AppState>, viewer: Viewer, Path((owner, repo)): Path<(String, String)>) -> WebResult<Redirect> {
    let (loaded, _) = load_mirror(&state, &viewer, &owner, &repo).await?;
    state.services.mirrors.sync_now(&loaded.ctx).await?;
    Ok(Redirect::to(&format!("{}/settings/mirror?queued=1", loaded.urls.home())))
}

/// `POST /{owner}/{repo}/settings/mirror/remove`: stop mirroring.
async fn remove(State(state): State<AppState>, viewer: Viewer, Path((owner, repo)): Path<(String, String)>) -> WebResult<Redirect> {
    let (loaded, _) = load_mirror(&state, &viewer, &owner, &repo).await?;
    state.services.mirrors.remove(&loaded.ctx).await?;
    Ok(Redirect::to(&format!("{}/settings", loaded.urls.home())))
}

#[cfg(test)]
mod tests {
    use super::suggested_name;

    #[test]
    fn names_are_suggested_from_the_url() {
        assert_eq!(suggested_name("https://example.com/group/tool.git"), "tool");
        assert_eq!(suggested_name("https://example.com/group/tool/"), "tool");
        assert_eq!(suggested_name("https://example.com/tool.git?x=1"), "tool");
    }
}
