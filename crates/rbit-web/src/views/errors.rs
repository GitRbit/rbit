//! The shared error page view model.

use askama::Template;
use askama_web::WebTemplate;
use axum::http::StatusCode;
use rbit_localization::{
    Localized,
    shell::pages::{Failure, PageMessage},
};

use crate::{error::ErrorInfo, views::layout::Page};

#[derive(Debug, Template, WebTemplate)]
#[template(path = "error.html")]
pub struct ErrorPage {
    pub page: Page,
    pub info: ErrorInfo,
}

impl ErrorPage {
    /// The failure the status stands for, when the catalog names it.
    const fn failure(&self) -> Option<Failure> {
        Some(match self.info.status {
            StatusCode::NOT_FOUND => Failure::NotFound,
            StatusCode::FORBIDDEN => Failure::Forbidden,
            StatusCode::BAD_REQUEST => Failure::BadRequest,
            StatusCode::TOO_MANY_REQUESTS => Failure::SlowDown,
            StatusCode::SERVICE_UNAVAILABLE => Failure::Unavailable,
            StatusCode::INTERNAL_SERVER_ERROR => Failure::Internal,
            _ => return None,
        })
    }

    /// The heading in the page's language, or the handler's English title.
    pub fn title(&self) -> Localized {
        match self.failure() {
            Some(failure) => self.page.text(PageMessage::ErrorTitle(failure)),
            None => Localized::english_only(self.info.title.to_owned(), self.page.content_locale),
        }
    }

    /// The explanation: catalog text for the fixed messages, otherwise the handler's
    /// own English message.
    pub fn message(&self) -> Localized {
        let fixed = match self.failure() {
            Some(Failure::NotFound) => Some(PageMessage::NotFoundText),
            Some(Failure::Unavailable) if self.info.message == crate::error::UNAVAILABLE_MESSAGE => Some(PageMessage::UnavailableText),
            Some(Failure::Internal) if self.info.message == crate::error::INTERNAL_MESSAGE => Some(PageMessage::InternalText),
            _ => None,
        };
        match fixed {
            Some(message) => self.page.text(message),
            None => Localized::english_only(self.info.message.clone(), self.page.content_locale),
        }
    }
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
