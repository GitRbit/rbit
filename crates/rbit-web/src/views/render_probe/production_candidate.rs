//! Independent inherited references exist only in test builds of the candidate.

use std::fmt;

use askama::{FastWritable, NO_VALUES, Template};

use super::{ExplorePage, LoginPage, Page, fixtures};
use crate::{
    themes::Theme,
    views::{components::FormError, explore::ExploreResults, external_auth::ProviderButton},
};

#[derive(Template)]
#[template(path = "home/explore.html")]
struct ExploreReference<'a> {
    page: &'a Page,
    query: &'a str,
    sort: &'static str,
    archived: Option<&'static str>,
    results: &'a ExploreResults,
}

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginReference<'a> {
    page: &'a Page,
    error: &'a FormError,
    login: &'a str,
    next: &'a str,
    registration_open: bool,
    providers: &'a [ProviderButton],
}

pub(super) fn explore_reference(view: &ExplorePage, output: &mut String) -> askama::Result<()> {
    ExploreReference { page: &view.page, query: &view.query, sort: view.sort, archived: view.archived, results: &view.results }
        .render_into(output)
}

pub(super) fn login_reference(view: &LoginPage, output: &mut String) -> askama::Result<()> {
    LoginReference {
        page: &view.page,
        error: &view.error,
        login: &view.login,
        next: &view.next,
        registration_open: view.registration_open,
        providers: &view.providers,
    }
    .render_into(output)
}

fn assert_entry_points(view: &impl Template, expected: &str) {
    assert_eq!(view.render_with_values(NO_VALUES).unwrap(), expected);
    let mut output = String::from("caller-prefix:");
    view.render_into_with_values(&mut output, NO_VALUES).unwrap();
    assert_eq!(output, format!("caller-prefix:{expected}"));
    output.clear();
    FastWritable::write_into(view, &mut output, NO_VALUES).unwrap();
    assert_eq!(output, expected);
    assert_eq!(view.to_string(), expected);
    let mut bytes = Vec::new();
    Template::write_into(view, &mut bytes).unwrap();
    assert_eq!(bytes, expected.as_bytes());
    struct Reject;
    impl fmt::Write for Reject {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    assert!(view.render_into(&mut Reject).is_err());
}

#[test]
fn actual_page_template_entry_points_preserve_bytes_and_writer_errors() {
    let explore = fixtures::explore(fixtures::Actor::Admin, Theme::Dark, rbit_localization::Locale::PseudoRtl, false);
    let mut expected = String::new();
    explore_reference(&explore, &mut expected).unwrap();
    assert_entry_points(&explore, &expected);
    let login = fixtures::login(fixtures::Actor::Anonymous, Theme::System, true);
    expected.clear();
    login_reference(&login, &mut expected).unwrap();
    assert_entry_points(&login, &expected);
}
