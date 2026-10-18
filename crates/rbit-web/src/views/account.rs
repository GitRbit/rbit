use askama::Template;
use askama_web::WebTemplate;

use crate::views::{
    components::{Avatar, RepoCard},
    layout::Page,
};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "account/profile.html")]
pub struct ProfilePage {
    pub contributions: super::contributions::ContributionCalendar,
    pub page: Page,
    pub name: String,
    pub label: String,
    pub bio: Option<String>,
    pub avatar: Avatar,
    pub joined: String,
    /// Always an `http(s)://` URL (validated when saved).
    pub website: Option<String>,
    pub location: Option<String>,
    /// `"repositories"` or `"stars"`.
    pub tab: &'static str,
    pub repo_count: usize,
    pub star_count: usize,
    /// The cards for the active tab.
    pub repos: Vec<RepoCard>,
    /// The viewer is looking at their own profile.
    pub is_self: bool,
}

impl ProfilePage {
    /// Displayed form of the website: the host and path without the scheme.
    pub fn website_label(&self) -> Option<&str> {
        self.website.as_deref().map(|url| url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/'))
    }
}
