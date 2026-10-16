//! The front page, the dashboard, explore and the "new repository" form.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rustit_core::RepoSort;
use rustit_services::instance::PublicCounts;

use crate::views::{
    components::{Avatar, FormError, OrgLink, Pagination, RepoCard},
    layout::Page,
};

/// The front page for anonymous visitors: what this instance is, a way into explore,
/// a few public numbers and the most recently updated public repositories.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "home/landing.html")]
pub struct LandingPage {
    pub page: Page,
    pub recent: Vec<RepoCard>,
    pub counts: PublicCounts,
}

/// One figure in the landing page's stats strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    /// Formatted with thousands separators, e.g. `12,345`.
    pub value: String,
    pub label: &'static str,
}

impl LandingPage {
    /// Public repositories, users and organisations, labelled in the singular or plural.
    pub fn stats(&self) -> [Stat; 3] {
        let counts = &self.counts;
        [
            stat(counts.public_repositories, "Repository", "Repositories"),
            stat(counts.users, "User", "Users"),
            stat(counts.organizations, "Organization", "Organizations"),
        ]
    }

    /// A brand-new instance: nobody has signed up yet, so the first account will be the
    /// administrator.
    pub fn is_fresh(&self) -> bool {
        self.counts.users == 0
    }
}

fn stat(count: i64, singular: &'static str, plural: &'static str) -> Stat {
    Stat { value: group_thousands(count), label: if count == 1 { singular } else { plural } }
}

/// `1234567` -> `1,234,567`.
pub fn group_thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        grouped.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
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

impl DashboardPage {
    /// How many of the viewer's repositories are private (shown beside the total).
    pub fn private_count(&self) -> usize {
        self.repos.iter().filter(|repo| repo.is_private).count()
    }

    /// The text the dashboard's instant filter matches against: `owner/name` and the
    /// description, lower-cased.
    pub fn filter_text(repo: &RepoCard) -> String {
        let mut text = format!("{}/{}", repo.owner, repo.name);
        if let Some(description) = &repo.description {
            text.push(' ');
            text.push_str(description);
        }
        text.to_lowercase()
    }
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

    fn page() -> Page {
        Page { site_name: "Rustit".to_owned(), title: None, viewer: None, path: "/".to_owned() }
    }

    fn landing(public_repositories: i64, users: i64, organizations: i64) -> LandingPage {
        LandingPage { page: page(), recent: Vec::new(), counts: PublicCounts { public_repositories, users, organizations } }
    }

    fn card(owner: &str, name: &str, description: Option<&str>, is_private: bool) -> RepoCard {
        RepoCard {
            owner: owner.to_owned(),
            name: name.to_owned(),
            url: format!("/{owner}/{name}"),
            description: description.map(str::to_owned),
            is_private,
            updated: "now".to_owned(),
            updated_iso: "2026-01-01T00:00:00Z".to_owned(),
            owner_avatar: Avatar { initials: "A".to_owned(), palette: 0 },
            star_count: 0,
        }
    }

    /// The page body, without the header (which has its own sign-in links).
    fn main_of(html: &str) -> &str {
        &html[html.find("<main").unwrap()..html.find("</main>").unwrap()]
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1,000");
        assert_eq!(group_thousands(1_234_567), "1,234,567");
        assert_eq!(group_thousands(-12_345), "-12,345");
    }

    #[test]
    fn stats_use_singular_labels_for_one() {
        let stats = landing(1, 2, 0).stats();
        let labels: Vec<_> = stats.iter().map(|stat| (stat.value.as_str(), stat.label)).collect();
        assert_eq!(labels, [("1", "Repository"), ("2", "Users"), ("0", "Organizations")]);
    }

    #[test]
    fn landing_page_has_no_sign_in_buttons_and_handles_a_fresh_instance() {
        let html = landing(0, 0, 0).render().unwrap();
        let main = main_of(&html);
        assert!(!main.contains(r#"href="/login""#) && !main.contains(r#"href="/signup""#), "the header already has them");
        assert!(main.contains(r#"action="/explore""#), "search goes to explore");
        assert!(main.contains("No public repositories yet"));
        assert!(main.contains("first account"), "a fresh instance explains who becomes administrator");

        let html = landing(3, 2, 1).render().unwrap();
        let main = main_of(&html);
        assert!(!main.contains("first account"));
        assert!(main.contains("Repositories") && main.contains("Organization"));
    }

    #[test]
    fn dashboard_counts_and_filter_text() {
        let repos = vec![card("alice", "Notes", Some("My PLANS"), true), card("acme", "site", None, false)];
        let dashboard = DashboardPage { page: page(), repos, recent: Vec::new(), orgs: Vec::new(), can_create_org: false };
        assert_eq!(dashboard.private_count(), 1);
        assert_eq!(DashboardPage::filter_text(&dashboard.repos[0]), "alice/notes my plans");
        assert_eq!(DashboardPage::filter_text(&dashboard.repos[1]), "acme/site");

        let html = dashboard.render().unwrap();
        assert!(html.contains(r#"data-filter-text="alice/notes my plans""#));
        assert!(html.contains(r#"data-filter="dashboard-repos""#));
    }

    #[test]
    fn explore_urls_omit_defaults_and_encode_queries() {
        assert_eq!(explore_url("", RepoSort::RecentlyUpdated, 1), "/explore");
        assert_eq!(explore_url("a&b c", RepoSort::Name, 2), "/explore?q=a%26b%20c&sort=name&page=2");
        assert_eq!(explore_url("", RepoSort::Newest, 0), "/explore?sort=newest");
    }
}
