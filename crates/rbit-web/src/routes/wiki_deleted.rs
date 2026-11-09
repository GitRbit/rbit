//! Deleted wiki pages: `W/-/deleted` lists them for readers, and writers restore one
//! with `POST W/-/page/{id}/undelete`.
//!
//! A deleted page keeps its history (readable at `W/-/page/{id}/history`) and comes
//! back with the content and names it had when it was deleted. Each restore form
//! carries the generation that deleted the page: restoring after the page changed again
//! goes back to the list, and a path another page took meanwhile is refused with `409`.
use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rbit_localization::{Localized, WikiMessage};
use rbit_services::wikis::errors::WikiError;
use rbit_wikis::WikiGeneration;
use serde::Deserialize;

use super::{
    Opened,
    edit::{BaseForm, base, writer},
    page_id, web_error,
};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    localization::RequestLocale,
    views::{
        layout::Page,
        wiki::dates,
        wiki_pages::{WikiDeletedRow, WikiDeletedTemplate},
    },
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DeletedQuery {
    before: String,
}

/// `GET W/-/deleted?before=`.
pub(super) async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<DeletedQuery>,
) -> WebResult<Response> {
    let before = if query.before.is_empty() {
        None
    } else {
        Some(query.before.parse::<i64>().ok().and_then(WikiGeneration::new).ok_or(WebError::NotFound)?)
    };
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let template = deleted_page(&state, &viewer, page, &opened, before, None).await?;
    Ok(opened.respond(template))
}

async fn deleted_page(
    state: &AppState,
    viewer: &Viewer,
    page: Page,
    opened: &Opened,
    before: Option<WikiGeneration>,
    error: Option<Localized>,
) -> WebResult<WikiDeletedTemplate> {
    let list = state.services.wikis.deleted(&opened.loaded.ctx, viewer.user(), before).await.map_err(web_error)?;
    let urls = &opened.urls;
    let entries = list
        .entries
        .iter()
        .map(|entry| {
            let (date, iso) = dates(entry.deleted_at, opened.text.locale());
            WikiDeletedRow {
                title: entry.title.to_string(),
                path: entry.path.to_string(),
                history_url: urls.action(entry.id, "history"),
                restore_url: urls.action(entry.id, "undelete"),
                base: entry.deleted.get(),
                deleted_by: entry.deleted_by.clone(),
                date,
                iso,
            }
        })
        .collect();
    let heading = opened.text.text(WikiMessage::DeletedPages);
    Ok(WikiDeletedTemplate {
        page: opened.page(page, heading.as_str()),
        header: opened.loaded.header(state, "wiki"),
        wiki: opened.chrome(state, viewer, None).await?,
        entries,
        error,
        older_url: list.next.map(|next| format!("{}?before={}", urls.deleted(), next.get())),
    })
}

/// `POST W/-/page/{id}/undelete`: to the restored page.
pub(super) async fn undelete(
    State(state): State<AppState>,
    user: SignedIn,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<BaseForm>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (opened, viewer) = writer(&state, &user, &owner, &repo, locale).await?;
    match state.services.wikis.undelete(&opened.loaded.ctx, &user.0, id, base(&form.base)?).await {
        Ok(path) => Ok(Redirect::to(&opened.urls.page(&path)).into_response()),
        // Restored or changed by someone else meanwhile: the list shows the current state.
        Err(WikiError::Conflict | WikiError::NotFound) => Ok(Redirect::to(&opened.urls.deleted()).into_response()),
        Err(WikiError::PathTaken) => {
            let message = opened.text.text(WikiMessage::RestoreBlocked);
            let template = deleted_page(&state, &viewer, page, &opened, None, Some(message)).await?;
            Ok(opened.respond((StatusCode::CONFLICT, template)))
        }
        Err(error) => Err(web_error(error)),
    }
}
