//! Borrowed feature bodies reuse the actual production view models.

use askama::Template;

use crate::views::{
    auth::LoginPage,
    components::FormError,
    explore::{ExplorePage, ExploreResults},
    external_auth::ProviderButton,
    layout::Page,
};

#[derive(Template)]
#[template(path = "render_probe/explore_body.html")]
pub(super) struct ExploreBody<'a> {
    query: &'a str,
    sort: &'static str,
    archived: Option<&'static str>,
    results: &'a ExploreResults,
}

impl<'a> From<&'a ExplorePage> for ExploreBody<'a> {
    fn from(view: &'a ExplorePage) -> Self {
        Self { query: &view.query, sort: view.sort, archived: view.archived, results: &view.results }
    }
}

#[derive(Template)]
#[template(path = "render_probe/login_body.html")]
pub(super) struct LoginBody<'a> {
    page: &'a Page,
    error: &'a FormError,
    login: &'a str,
    next: &'a str,
    registration_open: bool,
    providers: &'a [ProviderButton],
}

impl<'a> From<&'a LoginPage> for LoginBody<'a> {
    fn from(view: &'a LoginPage) -> Self {
        Self {
            page: &view.page,
            error: &view.error,
            login: &view.login,
            next: &view.next,
            registration_open: view.registration_open,
            providers: &view.providers,
        }
    }
}
