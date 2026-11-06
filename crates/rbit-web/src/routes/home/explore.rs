//! Public repository discovery with direct, localized live-search fragments.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_localization::{Catalog, Message};
use rbit_services::repositories::RepoSearch;
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    localization::{RequestLocale, single_header},
    views::{
        components::Pagination,
        explore::{ExploreFragment, ExploreLink, ExplorePage, ExploreResults, ExploreText, explore_url},
        layout::Page,
    },
};

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
    headers: HeaderMap,
    Query(params): Query<ExploreQuery>,
) -> WebResult<Response> {
    let query = params.q.unwrap_or_default().trim().to_owned();
    let sort = params.sort.as_deref().and_then(RepoSort::parse).unwrap_or_default();
    let archived = params.archived.as_deref().and_then(ArchivedFilter::parse).unwrap_or_default();
    let requested = params.page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(1);
    let search = RepoSearch { text: query.clone(), sort, archived, page: requested };
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
            label: catalog.text(match option {
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
            label: catalog.text(match option {
                ArchivedFilter::Include => Message::ArchiveAll,
                ArchivedFilter::Exclude => Message::ArchiveActive,
                ArchivedFilter::Only => Message::Archived,
            }),
            url: explore_url(&query, sort, option, 1),
            is_active: option == archived,
        })
        .collect();
    let labels = ExploreText::new(catalog, &page.site_name, found.total, &query, &pagination);
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
            labels,
        },
        query,
        sort: sort.as_str(),
        archived: archived.query_value(),
    };
    let response = if is_results_request(&headers) {
        ExploreFragment { page: &view.page, results: &view.results }.into_response()
    } else {
        view.into_response()
    };
    Ok(locale.response(response))
}

fn is_results_request(headers: &HeaderMap) -> bool {
    single_header(headers, "hx-request") == Some("true")
        && single_header(headers, "hx-target") == Some("explore-results")
        && !headers.contains_key("hx-history-restore-request")
        && !headers.contains_key("hx-boosted")
}
