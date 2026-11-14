//! The forks list and the fork form.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.

use askama::Template;
use askama_web::WebTemplate;

use crate::views::{
    components::{FormError, RepoCard},
    home::OwnerOption,
    layout::Page,
    repo::RepoHeader,
};

/// "forked from owner/name" under the repository title.
#[derive(Debug, Clone, Default)]
pub struct ForkedFrom {
    pub full_name: String,
    pub url: String,
    pub public: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/fork_sync.html")]
pub struct ForkSyncPanel {
    pub page: Page,
    pub sync: rbit_services::forks::ForkSync,
    pub action_url: String,
    pub compare_url: String,
}

/// Full page fallback when a request was not made by the deferred fragment.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/fork_sync_page.html")]
pub struct ForkSyncPage {
    pub page: Page,
    pub sync: rbit_services::forks::ForkSync,
    pub action_url: String,
    pub compare_url: String,
    pub home_url: String,
}

/// `/{owner}/{repo}/fork`: choose where the fork goes.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/fork.html")]
pub struct ForkFormPage {
    pub page: Page,
    pub header: RepoHeader,
    pub error: FormError,
    pub owners: Vec<OwnerOption>,
    pub name: String,
    pub action_url: String,
}

/// `/{owner}/{repo}/forks`: forks the viewer may read.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/forks.html")]
pub struct ForksPage {
    pub page: Page,
    pub header: RepoHeader,
    pub forks: Vec<RepoCard>,
    /// Shown above the list when the viewer is signed in.
    pub fork_url: Option<String>,
}
