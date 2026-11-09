//! Localized Explore presentation shared by full documents and direct fragments.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_localization::{Catalog, Localized, LocalizedDisplay, Message};

use super::{
    components::{Pagination, RepoCard},
    layout::Page,
};

/// One typed, localized filter/sort control; URLs retain stable machine values.
#[derive(Debug)]
pub struct ExploreLink {
    pub label: Localized,
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
    pub total: u64,
    pub labels: ExploreText,
}

/// A canonical document, including a usable ordinary GET search form.
#[derive(Debug, WebTemplate)]
#[cfg_attr(not(feature = "shared-shell-experiment"), derive(Template))]
#[cfg_attr(not(feature = "shared-shell-experiment"), template(path = "home/explore.html"))]
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
    pub query: &'a str,
    pub page: &'a Page,
    pub results: &'a ExploreResults,
}

/// Plain text for the feature-owned component, prepared once before rendering.
#[derive(Debug)]
pub struct ExploreText {
    pub title: Localized,
    pub search_placeholder: Localized,
    pub search_label: Localized,
    pub search_button: Localized,
    pub archive_filter: Localized,
    pub sort_by: Localized,
    pub empty_title: Localized,
    pub empty_description: Localized,
    pub no_match_title: Localized,
    pub no_match_description: Localized,
    pub archived: Localized,
    pub pagination: Localized,
    pub previous: Localized,
    pub next: Localized,
}

impl ExploreText {
    pub fn new(catalog: Catalog) -> Self {
        Self {
            title: catalog.localized(Message::ExploreTitle),
            search_placeholder: catalog.localized(Message::ExploreSearchPlaceholder),
            search_label: catalog.localized(Message::SearchRepositories),
            search_button: catalog.localized(Message::SearchLabel),
            archive_filter: catalog.localized(Message::ArchiveFilterLabel),
            sort_by: catalog.localized(Message::SortBy),
            empty_title: catalog.localized(Message::NothingHereYet),
            empty_description: catalog.localized(Message::PublicRepositoriesEmpty),
            no_match_title: catalog.localized(Message::NoMatchingRepositories),
            no_match_description: catalog.localized(Message::SearchSuggestion),
            archived: catalog.localized(Message::Archived),
            pagination: catalog.localized(Message::PaginationLabel),
            previous: catalog.localized(Message::PreviousPage),
            next: catalog.localized(Message::NextPage),
        }
    }
}

impl ExploreResults {
    pub fn count<'a>(&self, query: &'a str) -> LocalizedDisplay<'a> {
        Catalog::new(self.locale).display(Message::CountForQuery { count: self.total, query })
    }

    pub fn star_label(&self, count: &i64) -> LocalizedDisplay<'static> {
        Catalog::new(self.locale).display(Message::Stars { count: *count })
    }

    pub fn subtitle<'a>(&self, site: &'a str) -> LocalizedDisplay<'a> {
        Catalog::new(self.locale).display(Message::ExploreSubtitle { site })
    }

    pub fn page_of(&self) -> LocalizedDisplay<'static> {
        Catalog::new(self.locale).display(Message::PageOf { page: self.pagination.page, pages: self.pagination.page_count })
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
