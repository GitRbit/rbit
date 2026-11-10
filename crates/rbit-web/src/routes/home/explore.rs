//! Public repository discovery with direct, localized live-search fragments.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_localization::{Catalog, Message};
use rbit_services::repositories::RepoSearch;
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    localization::RequestLocale,
    views::{
        components::Pagination,
        explore::{ExploreFragment, ExploreLink, ExplorePage, ExploreResults, ExploreText, explore_url},
        layout::Page,
    },
};

mod response_shape;
use response_shape::ExploreRequestShape;

/// Bad machine values keep the existing lenient defaults, including archived/page.
#[derive(Debug, Deserialize)]
pub struct ExploreQuery {
    q: Option<String>,
    sort: Option<String>,
    archived: Option<String>,
    page: Option<String>,
}

pub async fn explore(
    State(state): State<AppState>,
    page: Page,
    locale: RequestLocale,
    shape: ExploreRequestShape,
    Query(params): Query<ExploreQuery>,
) -> WebResult<Response> {
    // Turned off by an administrator: gone for everyone, administrators included.
    if !state.services.instance.current().explore_enabled {
        return Err(WebError::NotFound);
    }
    let search = RepoSearch::from_query(params.q.as_deref(), params.sort.as_deref(), params.archived.as_deref(), params.page.as_deref());
    let (query, sort, archived) = (search.text.clone(), search.sort, search.archived);
    // This public-only service remains the sole authority for result visibility.
    let found = state.services.repositories.explore(&search).await?;
    let pagination = Pagination {
        page: found.page,
        page_count: found.page_count(),
        previous_url: found.has_previous().then(|| explore_url(&query, sort, archived, found.page - 1)),
        next_url: found.has_next().then(|| explore_url(&query, sort, archived, found.page + 1)),
    };
    let catalog = Catalog::new(locale.0.locale);
    let sorts = RepoSort::ALL
        .iter()
        .map(|&option| ExploreLink {
            label: catalog.localized(match option {
                RepoSort::RecentlyUpdated => Message::SortUpdated,
                RepoSort::Name => Message::SortName,
                RepoSort::Newest => Message::SortNewest,
            }),
            url: explore_url(&query, option, archived, 1),
            is_active: option == sort,
        })
        .collect();
    let archived_filters = ArchivedFilter::ALL
        .iter()
        .map(|&option| ExploreLink {
            label: catalog.localized(match option {
                ArchivedFilter::Include => Message::ArchiveAll,
                ArchivedFilter::Exclude => Message::ArchiveActive,
                ArchivedFilter::Only => Message::Archived,
            }),
            url: explore_url(&query, sort, option, 1),
            is_active: option == archived,
        })
        .collect();
    let labels = ExploreText::new(catalog);
    let title = if query.is_empty() { labels.title.to_string() } else { format!("{query} · {}", labels.title) };
    let view = ExplorePage {
        page: page.with_locale(locale.0.locale).titled(title),
        results: ExploreResults {
            locale: locale.0.locale,
            repos: super::super::listing::repo_cards(&state, &found.repos).await?,
            sorts,
            archived_filters,
            pagination,
            query_is_empty: query.is_empty(),
            total: found.total,
            labels,
        },
        query,
        sort: sort.as_str(),
        archived: archived.query_value(),
    };
    let response = if shape.results_only() {
        ExploreFragment { page: &view.page, query: &view.query, results: &view.results }.into_response()
    } else {
        view.into_response()
    };
    Ok(locale.response(response))
}
