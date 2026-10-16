//! The front page, the dashboard, explore and the "new repository" form.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rustit_core::RepoSort;

use crate::views::{
    components::{Avatar, FormError, OrgLink, Pagination, RepoCard},
    layout::Page,
};

/// The front page for anonymous visitors.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/landing.html")]
pub struct LandingPage {
    pub page: Page,
    pub recent: Vec<RepoCard>,
}

/// The front page for signed-in users.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/dashboard.html")]
pub struct DashboardPage {
    pub page: Page,
    /// The viewer's repositories and their organisations'.
    pub repos: Vec<RepoCard>,
    pub recent: Vec<RepoCard>,
    pub orgs: Vec<OrgLink>,
    /// Show "New organization" (the instance may limit creation to admins).
    pub can_create_org: bool,
}

// ------------------------------------------------------------------ explore

/// One sort order in the explore list header.
#[derive(Debug, Clone)]
pub struct SortLink {
    pub label: &'static str,
    pub url: String,
    pub is_active: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/explore.html")]
pub struct ExplorePage {
    pub page: Page,
    pub repos: Vec<RepoCard>,
    /// The search text as typed (re-filled into the search box).
    pub query: String,
    /// The active sort's query-string value, kept when searching again.
    pub sort: &'static str,
    pub sorts: Vec<SortLink>,
    pub total: u64,
    pub pagination: Pagination,
}

impl ExplorePage {
    /// "3 repositories", "1 repository".
    pub fn total_label(&self) -> String {
        let noun = if self.total == 1 { "repository" } else { "repositories" };
        format!("{} {noun}", self.total)
    }
}

/// `/explore?q=…&sort=…&page=…`, omitting defaults so canonical URLs stay short.
pub fn explore_url(query: &str, sort: RepoSort, page: u32) -> String {
    let mut params = Vec::new();
    if !query.is_empty() {
        params.push(format!("q={}", utf8_percent_encode(query, NON_ALPHANUMERIC)));
    }
    if sort != RepoSort::default() {
        params.push(format!("sort={}", sort.as_str()));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() { "/explore".to_owned() } else { format!("/explore?{}", params.join("&")) }
}

/// Short labels for the list header (fits a phone screen).
pub const fn sort_label(sort: RepoSort) -> &'static str {
    match sort {
        RepoSort::RecentlyUpdated => "Updated",
        RepoSort::Name => "Name",
        RepoSort::Newest => "Newest",
    }
}

// ------------------------------------------------------------------ new repository

/// An account a new repository can belong to: the viewer or one of their organisations.
#[derive(Debug, Clone)]
pub struct OwnerOption {
    pub name: String,
    pub avatar: Avatar,
    pub selected: bool,
}

/// `/new`: create a repository for yourself or an organisation.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/new.html")]
pub struct NewRepositoryPage {
    pub page: Page,
    pub error: FormError,
    /// The viewer first, then their organisations. Exactly one is `selected`.
    pub owners: Vec<OwnerOption>,
    pub name: String,
    pub description: String,
    pub private: bool,
}

impl NewRepositoryPage {
    /// The selected owner (for the single-owner layout).
    pub fn selected_owner(&self) -> Option<&OwnerOption> {
        self.owners.iter().find(|owner| owner.selected).or_else(|| self.owners.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_urls_omit_defaults_and_encode_queries() {
        assert_eq!(explore_url("", RepoSort::RecentlyUpdated, 1), "/explore");
        assert_eq!(explore_url("a&b c", RepoSort::Name, 2), "/explore?q=a%26b%20c&sort=name&page=2");
        assert_eq!(explore_url("", RepoSort::Newest, 0), "/explore?sort=newest");
    }
}
