//! Localized Explore presentation shared by full documents and direct fragments.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_localization::{Catalog, LocalizedText, Message};

use super::{
    components::{Pagination, RepoCard},
    layout::Page,
};

/// One typed, localized filter/sort control; URLs retain stable machine values.
#[derive(Debug)]
pub struct ExploreLink {
    pub label: LocalizedText,
    pub url: String,
    pub is_active: bool,
}

/// The same results component is included in full and fragment templates.
#[derive(Debug)]
pub struct ExploreResults {
    pub locale: rbit_localization::Locale,
    pub repos: Vec<RepoCard>,
    pub sorts: Vec<ExploreLink>,
    pub archived_filters: Vec<ExploreLink>,
    pub pagination: Pagination,
    pub query_is_empty: bool,
    pub labels: ExploreText,
}

/// A canonical document, including a usable ordinary GET search form.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/explore.html")]
pub struct ExplorePage {
    pub page: Page,
    pub query: String,
    pub sort: &'static str,
    pub archived: Option<&'static str>,
    pub results: ExploreResults,
}

/// An explicit live-search response: title plus the exact full-page results component.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/explore_fragment.html")]
pub struct ExploreFragment<'a> {
    pub page: &'a Page,
    pub results: &'a ExploreResults,
}

/// Plain text for the feature-owned component, prepared once before rendering.
#[derive(Debug)]
pub struct ExploreText {
    pub title: LocalizedText,
    pub subtitle: LocalizedText,
    pub search_placeholder: LocalizedText,
    pub search_label: LocalizedText,
    pub search_button: LocalizedText,
    pub archive_filter: LocalizedText,
    pub sort_by: LocalizedText,
    pub count: LocalizedText,
    pub empty_title: LocalizedText,
    pub empty_description: LocalizedText,
    pub no_match_title: LocalizedText,
    pub no_match_description: LocalizedText,
    pub archived: LocalizedText,
    pub pagination: LocalizedText,
    pub previous: LocalizedText,
    pub next: LocalizedText,
    pub page_of: LocalizedText,
}

impl ExploreText {
    pub fn new(catalog: Catalog, site: &str, total: u64, query: &str, pager: &Pagination) -> Self {
        Self {
            title: catalog.text(Message::ExploreTitle),
            subtitle: catalog.text(Message::ExploreSubtitle { site }),
            search_placeholder: catalog.text(Message::ExploreSearchPlaceholder),
            search_label: catalog.text(Message::SearchRepositories),
            search_button: catalog.text(Message::SearchLabel),
            archive_filter: catalog.text(Message::ArchiveFilterLabel),
            sort_by: catalog.text(Message::SortBy),
            count: catalog.text(Message::CountForQuery { count: total, query }),
            empty_title: catalog.text(Message::NothingHereYet),
            empty_description: catalog.text(Message::PublicRepositoriesEmpty),
            no_match_title: catalog.text(Message::NoMatchingRepositories),
            no_match_description: catalog.text(Message::SearchSuggestion),
            archived: catalog.text(Message::Archived),
            pagination: catalog.text(Message::PaginationLabel),
            previous: catalog.text(Message::PreviousPage),
            next: catalog.text(Message::NextPage),
            page_of: catalog.text(Message::PageOf { page: pager.page, pages: pager.page_count }),
        }
    }
}

impl ExploreResults {
    pub fn star_label(&self, count: &i64) -> LocalizedText {
        Catalog::new(self.locale).text(Message::Stars { count: *count })
    }
}

/// `/explore?q=…&sort=…&archived=…&page=…`, omitting defaults so canonical URLs stay short.
pub fn explore_url(query: &str, sort: RepoSort, archived: ArchivedFilter, page: u32) -> String {
    let mut params = Vec::new();
    if !query.is_empty() {
        params.push(format!("q={}", utf8_percent_encode(query, NON_ALPHANUMERIC)));
    }
    if sort != RepoSort::default() {
        params.push(format!("sort={}", sort.as_str()));
    }
    if let Some(value) = archived.query_value() {
        params.push(format!("archived={value}"));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() { "/explore".to_owned() } else { format!("/explore?{}", params.join("&")) }
}
