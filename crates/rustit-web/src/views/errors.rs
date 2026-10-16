use askama::Template;
use askama_web::WebTemplate;

use crate::{error::ErrorInfo, views::layout::Page};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "error.html")]
pub struct ErrorPage {
    pub page: Page,
    pub info: ErrorInfo,
}
