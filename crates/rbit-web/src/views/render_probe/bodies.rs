//! Explicitly approved HTML-derived bodies; no blanket template or String adapter.

use askama::Template;

use super::{ApprovedBody, ExplorePage, LoginPage, Page, sealed};
use crate::views::{components::FormError, explore::ExploreResults, external_auth::ProviderButton};

macro_rules! explore_body {
    ($name:ident, $($template:tt)*) => {
        #[derive(Debug, Template)]
        #[template($($template)*)]
        pub(super) struct $name<'a> {
            query: &'a str,
            sort: &'static str,
            archived: Option<&'static str>,
            results: &'a ExploreResults,
        }

        impl<'a> From<&'a ExplorePage> for $name<'a> {
            fn from(view: &'a ExplorePage) -> Self {
                Self { query: &view.query, sort: view.sort, archived: view.archived, results: &view.results }
            }
        }
    };
}

macro_rules! login_body {
    ($name:ident, $($template:tt)*) => {
        #[derive(Debug, Template)]
        #[template($($template)*)]
        pub(super) struct $name<'a> {
            page: &'a Page,
            error: &'a FormError,
            login: &'a str,
            next: &'a str,
            registration_open: bool,
            providers: &'a [ProviderButton],
        }

        impl<'a> From<&'a LoginPage> for $name<'a> {
            fn from(view: &'a LoginPage) -> Self {
                Self { page: &view.page, error: &view.error, login: &view.login, next: &view.next, registration_open: view.registration_open, providers: &view.providers }
            }
        }
    };
}

explore_body!(ExploreSegments, path = "render_probe/explore_body.html");
explore_body!(ExploreBlocks, path = "home/explore.html", block = "content");
login_body!(LoginSegments, path = "render_probe/login_body.html");
login_body!(LoginBlocks, path = "auth/login.html", block = "content");

macro_rules! approved {
    ($($name:ident),+ $(,)?) => {$(
        impl sealed::Approved for $name<'_> {}
        impl ApprovedBody for $name<'_> {
            fn render_body(&self, output: &mut dyn std::fmt::Write) -> askama::Result<()> {
                Template::render_into(self, output)
            }
        }
    )+};
}

approved!(ExploreSegments, ExploreBlocks, LoginSegments, LoginBlocks);
