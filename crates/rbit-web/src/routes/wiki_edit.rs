//! Wiki changes: create, edit (and rename), restore, delete, and turning the wiki on/off.
//!
//! Anonymous requests are sent to sign in first; readers without write access get 403,
//! archived repositories refuse every change. Forms carry the whole-wiki generation they
//! were read at (`base`): saving after someone else changed the same page re-renders
//! the form with the submitted text and `409`; invalid fields re-render with `422`.
//! The preview button re-renders the form with the page rendered by the same safe
//! Markdown renderer as saved pages, without saving anything.
use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rbit_localization::{Localized, WikiMessage};
use rbit_services::wikis::{errors::WikiError, publish::WikiPageInput};
use rbit_wikis::{WikiGeneration, WikiPageId, WikiPath};
use serde::Deserialize;

use super::{Opened, page_id, web_error};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    localization::RequestLocale,
    render::render_markdown,
    views::{
        layout::Page,
        wiki_pages::{WikiConflictLinks, WikiDeleteTemplate, WikiEditTemplate},
    },
};

/// Opens the repository for a signed-in writer: 404 when unreadable or the wiki is
/// off, 403 without write access, refused when archived.
pub(super) async fn writer(
    state: &AppState,
    user: &SignedIn,
    owner: &str,
    repo: &str,
    locale: RequestLocale,
) -> WebResult<(Opened, Viewer)> {
    let viewer = Viewer(Some(user.0.clone()));
    let opened = Opened::load(state, &viewer, owner, repo, locale).await?;
    if opened.status(state).await? == rbit_services::wikis::pages::WikiStatus::Off {
        return Err(WebError::NotFound);
    }
    if !opened.loaded.ctx.access.can_write() {
        return Err(WebError::Forbidden);
    }
    opened.loaded.ctx.ensure_writable()?;
    Ok((opened, viewer))
}

pub(super) fn base(value: &str) -> WebResult<WikiGeneration> {
    value.parse::<i64>().ok().and_then(WikiGeneration::new).ok_or_else(|| WebError::BadRequest("invalid form".into()))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PageForm {
    title: String,
    path: String,
    content: String,
    base: String,
    intent: String,
}

impl PageForm {
    fn input(&self) -> WikiPageInput {
        WikiPageInput { title: self.title.clone(), path: self.path.clone(), content: self.content.clone() }
    }
}

/// The edit form with what the user typed.
struct Draft<'a> {
    form: &'a PageForm,
    error: Option<Localized>,
    conflict: Option<WikiConflictLinks>,
    preview: bool,
}

async fn edit_page(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    opened: &Opened,
    id: Option<WikiPageId>,
    draft: Draft<'_>,
) -> WebResult<WikiEditTemplate> {
    let text = opened.text;
    let heading = text.text(if id.is_some() { WikiMessage::EditPage } else { WikiMessage::NewPage });
    let (action_url, cancel_url) = match id {
        Some(id) => (
            opened.urls.action(id, "edit"),
            draft.conflict.as_ref().map_or_else(|| opened.urls.action(id, "history"), |c| c.current_url.clone()),
        ),
        None => (opened.urls.new_page(), opened.urls.home.clone()),
    };
    Ok(WikiEditTemplate {
        page: opened.page(page, heading.as_str()),
        header: opened.loaded.header(state, "wiki"),
        wiki: opened.chrome(state, viewer, id).await?,
        heading,
        action_url,
        cancel_url,
        title: draft.form.title.clone(),
        path: draft.form.path.clone(),
        content: draft.form.content.clone(),
        base: if id.is_some() { draft.form.base.parse().ok() } else { None },
        error: draft.error,
        conflict: draft.conflict,
        preview: draft.preview.then(|| render_markdown(&draft.form.content, None)),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewQuery {
    path: String,
}

/// `GET W/-/new?path=`.
pub(super) async fn new_form(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<NewQuery>,
) -> WebResult<Response> {
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    let title =
        query.path.parse::<WikiPath>().ok().map(|path| path.as_str().rsplit('/').next().unwrap_or_default().replace(['-', '_'], " "));
    let form = PageForm { title: title.unwrap_or_default(), path: query.path.chars().take(512).collect(), ..PageForm::default() };
    let template =
        edit_page(&state, &viewer, page, &opened, None, Draft { form: &form, error: None, conflict: None, preview: false }).await?;
    Ok(opened.respond(template))
}

/// Re-renders a refused form, or fails the request for errors that are not about it.
async fn refused(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    opened: &Opened,
    id: Option<WikiPageId>,
    form: &PageForm,
    error: WikiError,
) -> WebResult<Response> {
    let Some(message) = opened.text.form_error(&error) else { return Err(web_error(error)) };
    let status = match error {
        WikiError::Conflict | WikiError::PathTaken => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    let conflict = match (error, id) {
        (WikiError::Conflict, Some(id)) => Some(conflict_links(state, viewer, opened, id, form).await?),
        _ => None,
    };
    let template = edit_page(state, viewer, page, opened, id, Draft { form, error: Some(message), conflict, preview: false }).await?;
    Ok(opened.respond((status, template)))
}

/// Links to the current page and to the changes made since the form's `base`.
async fn conflict_links(
    state: &AppState,
    viewer: &Viewer,
    opened: &Opened,
    id: WikiPageId,
    form: &PageForm,
) -> WebResult<WikiConflictLinks> {
    let wikis = &state.services.wikis;
    let current = match wikis.page_by_id(&opened.loaded.ctx, viewer.user(), id).await {
        Ok(current) => current,
        // Deleted meanwhile: its history and restore are with the deleted pages.
        Err(WikiError::NotFound) => return Ok(WikiConflictLinks { current_url: opened.urls.deleted(), compare_url: None }),
        Err(error) => return Err(web_error(error)),
    };
    let edited = match form.base.parse::<i64>().ok().and_then(|value| value.checked_add(1)).and_then(WikiGeneration::new) {
        Some(after_base) => wikis.previous(&opened.loaded.ctx, viewer.user(), id, after_base).await.map_err(web_error)?,
        None => None,
    };
    let latest = wikis
        .history(&opened.loaded.ctx, viewer.user(), id, None)
        .await
        .map_err(web_error)?
        .entries
        .first()
        .map(|entry| entry.head.generation);
    let compare_url = match (edited, latest) {
        (Some(edited), Some(latest)) if edited != latest => Some(opened.urls.compare(id, edited.get(), latest.get())),
        _ => None,
    };
    Ok(WikiConflictLinks { current_url: opened.urls.page(&current.page.path), compare_url })
}

/// `POST W/-/new`.
pub(super) async fn create(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<PageForm>,
) -> WebResult<Response> {
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    if form.intent == "preview" {
        let template =
            edit_page(&state, &viewer, page, &opened, None, Draft { form: &form, error: None, conflict: None, preview: true }).await?;
        return Ok(opened.respond(template));
    }
    match state.services.wikis.create(&opened.loaded.ctx, &user.0, &form.input()).await {
        Ok(path) => Ok(Redirect::to(&opened.urls.page(&path)).into_response()),
        Err(error) => refused(&state, &viewer, page, &opened, None, &form, error).await,
    }
}

/// `GET W/-/page/{id}/edit`.
pub(super) async fn edit_form(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    let current = state.services.wikis.page_by_id(&opened.loaded.ctx, viewer.user(), id).await.map_err(web_error)?;
    let form = PageForm {
        title: current.page.title.to_string(),
        path: current.page.path.to_string(),
        content: current.content.as_str().to_owned(),
        base: current.page.head.generation.get().to_string(),
        intent: String::new(),
    };
    let template =
        edit_page(&state, &viewer, page, &opened, Some(id), Draft { form: &form, error: None, conflict: None, preview: false }).await?;
    Ok(opened.respond(template))
}

/// `POST W/-/page/{id}/edit`.
pub(super) async fn update(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<PageForm>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    let expected = base(&form.base)?;
    if form.intent == "preview" {
        let template =
            edit_page(&state, &viewer, page, &opened, Some(id), Draft { form: &form, error: None, conflict: None, preview: true }).await?;
        return Ok(opened.respond(template));
    }
    match state.services.wikis.edit(&opened.loaded.ctx, &user.0, id, expected, &form.input()).await {
        Ok(path) => Ok(Redirect::to(&opened.urls.page(&path)).into_response()),
        Err(error) => refused(&state, &viewer, page, &opened, Some(id), &form, error).await,
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BaseForm {
    pub(super) base: String,
}

/// `GET W/-/page/{id}/delete`: the confirmation.
pub(super) async fn delete_form(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    let current = state.services.wikis.page_by_id(&opened.loaded.ctx, viewer.user(), id).await.map_err(web_error)?;
    let heading = opened.text.text(WikiMessage::DeleteTitle { title: current.page.title.as_str() });
    let template = WikiDeleteTemplate {
        page: opened.page(page, heading.as_str()),
        header: opened.loaded.header(&state, "wiki"),
        wiki: opened.chrome(&state, &viewer, Some(id)).await?,
        heading,
        action_url: opened.urls.action(id, "delete"),
        cancel_url: opened.urls.page(&current.page.path),
        base: current.page.head.generation.get(),
    };
    Ok(opened.respond(template))
}

/// `POST W/-/page/{id}/delete`: back to the wiki home (the page's history is kept).
pub(super) async fn delete(
    State(state): State<AppState>,
    user: SignedIn,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<BaseForm>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (opened, _) = writer(&state, &user, &owner, &repo, locale).await?;
    match state.services.wikis.delete(&opened.loaded.ctx, &user.0, id, base(&form.base)?).await {
        Ok(()) => Ok(Redirect::to(&opened.urls.home).into_response()),
        Err(WikiError::Conflict) => Ok(Redirect::to(&opened.urls.action(id, "history")).into_response()),
        Err(error) => Err(web_error(error)),
    }
}

/// `POST W/-/page/{id}/revisions/{generation}/restore`.
pub(super) async fn restore(
    State(state): State<AppState>,
    user: SignedIn,
    locale: RequestLocale,
    Path((owner, repo, id, number)): Path<(String, String, String, String)>,
    Form(form): Form<BaseForm>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let number = number.parse::<i64>().ok().and_then(WikiGeneration::new).ok_or(WebError::NotFound)?;
    let (opened, _) = writer(&state, &user, &owner, &repo, locale).await?;
    match state.services.wikis.restore(&opened.loaded.ctx, &user.0, id, number, base(&form.base)?).await {
        Ok(path) => Ok(Redirect::to(&opened.urls.page(&path)).into_response()),
        // Stale or no-op restores show the history, which reflects the current state.
        Err(WikiError::Conflict | WikiError::Unchanged) => Ok(Redirect::to(&opened.urls.action(id, "history")).into_response()),
        Err(error) => Err(web_error(error)),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SettingsForm {
    enabled: String,
}

/// `POST W/-/settings`: `enabled=on|off`, repository administrators only.
pub(super) async fn settings(
    State(state): State<AppState>,
    user: SignedIn,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<SettingsForm>,
) -> WebResult<Response> {
    let viewer = Viewer(Some(user.0.clone()));
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let enabled = match form.enabled.as_str() {
        "on" => true,
        "off" => false,
        _ => return Err(WebError::BadRequest("invalid form".into())),
    };
    state.services.wikis.set_enabled(&ctx, enabled).await.map_err(web_error)?;
    let home = crate::urls::RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str()).home();
    Ok(Redirect::to(&if enabled { format!("{home}/wiki") } else { home }).into_response())
}
