//! Wiki page history, older revisions and comparisons (`W/-/page/{id}/…`).
use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use rbit_localization::WikiMessage;
use rbit_services::wikis::pages::WikiPageState;
use rbit_wikis::{WikiGeneration, change::WikiChange, reading::WikiHistoryEntry};
use serde::Deserialize;

use super::{Opened, page_id, web_error};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    localization::RequestLocale,
    views::{
        layout::Page,
        wiki::{WikiDiffView, dates},
        wiki_pages::{WikiCompareTemplate, WikiHistoryRow, WikiHistoryTemplate, WikiRevisionNote},
    },
};

fn generation(value: &str) -> WebResult<WikiGeneration> {
    value.parse::<i64>().ok().and_then(WikiGeneration::new).ok_or(WebError::NotFound)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct HistoryQuery {
    before: String,
}

/// `GET W/-/page/{id}/history?before=`.
pub(super) async fn history(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Query(query): Query<HistoryQuery>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let before = if query.before.is_empty() { None } else { Some(generation(&query.before)?) };
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let wikis = &state.services.wikis;
    let state_now = wikis.page_state(&opened.loaded.ctx, viewer.user(), id).await.map_err(web_error)?;
    let history = wikis.history(&opened.loaded.ctx, viewer.user(), id, before).await.map_err(web_error)?;
    // Each revision compares with the next older one; the last of this page needs the
    // first entry of the following page, which is one more (bounded) lookup.
    let older = match history.entries.last() {
        Some(last) if history.next.is_some() => {
            wikis.previous(&opened.loaded.ctx, viewer.user(), id, last.head.generation).await.map_err(web_error)?
        }
        _ => None,
    };
    let text = opened.text;
    let urls = &opened.urls;
    // A deletion has no content: it is neither viewed nor compared.
    let content = |entry: &WikiHistoryEntry| entry.change != Some(WikiChange::Delete);
    let previous = history.entries.iter().skip(1).map(|entry| content(entry).then_some(entry.head.generation)).chain([older]);
    let entries = history
        .entries
        .iter()
        .zip(previous)
        .map(|(entry, previous)| {
            let generation = entry.head.generation.get();
            let (date, iso) = dates(entry.at);
            let shown = content(entry);
            WikiHistoryRow {
                label: text.text(WikiMessage::Revision { generation }),
                url: shown.then(|| urls.revision(id, generation)),
                renamed_from: entry.renamed_from.as_ref().map(|from| text.text(WikiMessage::RenamedFrom { path: from.as_str() })),
                title: entry.title.to_string(),
                change: text.change(entry.change),
                author: entry.author.clone(),
                date,
                iso,
                compare_url: previous.filter(|_| shown).map(|previous| urls.compare(id, previous.get(), generation)),
            }
        })
        .collect();
    let (title, page_url, deleted_notice) = match &state_now {
        WikiPageState::Current(current) => (current.page.title.as_str(), urls.page(&current.page.path), None),
        WikiPageState::Deleted(deleted) => (deleted.title.as_str(), urls.deleted(), Some(text.text(WikiMessage::DeletedNotice))),
    };
    let template = WikiHistoryTemplate {
        page: opened.page(page, text.text(WikiMessage::HistoryOf { title }).as_str()),
        header: opened.loaded.header(&state, "wiki"),
        wiki: opened.chrome(&state, &viewer, Some(id)).await?,
        heading: text.text(WikiMessage::HistoryOf { title }),
        page_url,
        deleted_notice,
        entries,
        older_url: history.next.map(|next| format!("{}?before={}", urls.action(id, "history"), next.get())),
    };
    Ok(opened.respond(template))
}

/// `GET W/-/page/{id}/revisions/{generation}`: the page as it was, with a restore form.
pub(super) async fn revision(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id, number)): Path<(String, String, String, String)>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let number = generation(&number)?;
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let wikis = &state.services.wikis;
    let (old, current, previous) = tokio::try_join!(
        wikis.revision(&opened.loaded.ctx, viewer.user(), id, number),
        wikis.page_state(&opened.loaded.ctx, viewer.user(), id),
        wikis.previous(&opened.loaded.ctx, viewer.user(), id, number),
    )
    .map_err(web_error)?;
    // A deleted page is restored as a whole from the deleted pages, not per revision.
    let (current_url, restore_url, base) = match current {
        WikiPageState::Current(current) => (
            opened.urls.page(&current.page.path),
            Some(format!("{}/restore", opened.urls.revision(id, number.get()))),
            current.page.head.generation.get(),
        ),
        WikiPageState::Deleted(deleted) => (opened.urls.deleted(), None, deleted.deleted.get()),
    };
    let note = WikiRevisionNote {
        label: opened.text.text(WikiMessage::Revision { generation: number.get() }),
        current_url,
        restore_url,
        base,
        compare_url: previous.map(|previous| opened.urls.compare(id, previous.get(), number.get())),
    };
    let chrome = opened.chrome(&state, &viewer, Some(id)).await?;
    Ok(opened.respond(opened.page_view(page, chrome, &old, Some(note), &state)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CompareQuery {
    base: String,
    head: String,
}

/// `GET W/-/page/{id}/compare?base=&head=`.
pub(super) async fn compare(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Query(query): Query<CompareQuery>,
) -> WebResult<Response> {
    let id = page_id(&id)?;
    let (base, head) = (generation(&query.base)?, generation(&query.head)?);
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let comparison = state.services.wikis.compare(&opened.loaded.ctx, viewer.user(), id, base, head).await.map_err(web_error)?;
    let text = opened.text;
    let changes = WikiDiffView::new(comparison.changes);
    let template = WikiCompareTemplate {
        page: opened.page(page, comparison.head.page.title.as_str()),
        header: opened.loaded.header(&state, "wiki"),
        wiki: opened.chrome(&state, &viewer, Some(id)).await?,
        title: comparison.head.page.title.to_string(),
        heading: text.text(WikiMessage::Comparing { base: base.get(), head: head.get() }),
        summary: text.text(WikiMessage::Changes { additions: changes.additions, deletions: changes.deletions }),
        changes,
        history_url: opened.urls.action(id, "history"),
    };
    Ok(opened.respond(template))
}
