//! Host shells for markup produced exclusively by the compiled Askama bridge.

use askama::Template;
use askama_web::WebTemplate;
use rbit_services::plugin_pages::PageBody;

use super::{layout::Page, repo::RepoHeader};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "plugin_pages/global.html")]
pub struct GlobalPluginPage {
    pub page: Page,
    pub owner: String,
    pub route: String,
    pub body: PageBody,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "plugin_pages/repository.html")]
pub struct RepositoryPluginPage {
    pub page: Page,
    pub header: RepoHeader,
    pub owner: String,
    pub route: String,
    pub body: PageBody,
}
