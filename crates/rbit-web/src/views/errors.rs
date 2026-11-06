//! The shared error page view model.

use askama::Template;
use askama_web::WebTemplate;

use crate::{error::ErrorInfo, views::layout::Page};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "error.html")]
pub struct ErrorPage {
    pub page: Page,
    pub info: ErrorInfo,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn generic_error_pages_suppress_history_even_when_the_request_path_contains_a_live_secret() {
        for status in [StatusCode::BAD_REQUEST, StatusCode::INTERNAL_SERVER_ERROR, StatusCode::SERVICE_UNAVAILABLE] {
            let page = crate::views::test_support::page("/password/reset/live-secret");
            let info = ErrorInfo { status, title: "Temporarily unavailable", message: crate::error::UNAVAILABLE_MESSAGE.into() };
            let html = ErrorPage { page, info }.render().unwrap();
            assert!(html.contains("hx-history=\"false\""), "error substitution must retain the sensitive-page history barrier");
            assert!(html.contains("/password/reset/live-secret"), "the shared chrome includes the current path");
        }
    }
}
