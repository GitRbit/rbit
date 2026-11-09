//! Full documents in every catalog: the headers name the language and catalog the body is
//! rendered in, hostile text is escaped once, and the CSP stays strict.
use axum::http::StatusCode;
use rbit_localization::Locale;

use super::fixture::{ESCAPED, Fixture, HOSTILE, LOCALES, Page, REPO};

/// Pages with hostile user text, and pages without.
fn pages() -> Vec<(String, bool)> {
    let mut pages: Vec<(String, bool)> = [
        "/explore",
        REPO,
        "/safety-owner/demo/issues",
        "/safety-owner/demo/issues/1",
        "/safety-owner/demo/pulls",
        "/safety-owner/demo/pull/2",
    ]
    .into_iter()
    .map(|path| (path.to_owned(), true))
    .collect();
    pages.extend(["/", "/login", "/safety-owner"].into_iter().map(|path| (path.to_owned(), false)));
    pages
}

fn assert_language_matches_body(page: &Page, context: &str) {
    let language = page.header("content-language");
    let catalog = page.header("rbit-catalog-key");
    assert!(!language.is_empty() && !catalog.is_empty(), "{context}: {:?}", page.headers);
    assert_eq!(page.attribute("lang"), Some(language), "{context}: <html lang> and Content-Language");
    assert_eq!(page.attribute("data-catalog-key"), Some(catalog), "{context}: <html> catalog and header");
    assert_eq!(page.attribute("data-document-catalog"), Some(catalog), "{context}: the site header's catalog");
    assert_eq!(catalog.split_once('.').map(|(_, tag)| tag), Some(language), "{context}: one locale for both headers");
    let direction = if language == Locale::PseudoRtl.tag() { "rtl" } else { "ltr" };
    assert_eq!(page.attribute("dir"), Some(direction), "{context}");
    let vary = page.header("vary");
    for dimension in ["Accept-Language", "Cookie", "Rbit-Document-Locale"] {
        assert!(vary.contains(dimension), "{context}: Vary lacks {dimension}: {vary}");
    }
}

/// No inline script, handler or style attribute; the only `<style>` elements are the
/// embedded stylesheets whose hashes the policy lists.
fn assert_strict_csp(page: &Page, context: &str) {
    let policy = page.header("content-security-policy");
    assert!(policy.contains("script-src 'self';") && !policy.contains("unsafe-inline"), "{context}: {policy}");
    for tag in page.body.match_indices("<script").map(|(at, _)| &page.body[at..at + page.body[at..].find('>').unwrap()]) {
        assert!(tag.contains(" src=\""), "{context}: inline script {tag}");
    }
    assert!(!page.body.contains(" style=\""), "{context}: a style attribute");
    for (at, _) in page.body.match_indices("<style") {
        let tag = &page.body[at..at + page.body[at..].find('>').unwrap()];
        assert!(tag.contains("data-app-stylesheet") || tag.contains("data-theme-stylesheet"), "{context}: {tag}");
    }
    let tags = page.body.split('<').skip(1).map(|tag| tag.split('>').next().unwrap_or_default());
    for tag in tags.filter(|tag| !tag.starts_with('!')) {
        let handler = tag.split_whitespace().skip(1).any(|attribute| attribute.starts_with("on") && attribute.contains('='));
        assert!(!handler, "{context}: an event handler in <{tag}>");
    }
}

fn assert_escaped_once(page: &Page, context: &str) {
    assert!(!page.body.contains(HOSTILE.split(" &").next().unwrap()), "{context}: unescaped markup");
    assert!(page.body.contains(ESCAPED), "{context}: hostile text is shown escaped once");
    assert!(!page.body.contains("&#38;#"), "{context}: something was escaped twice");
}

#[tokio::test]
async fn every_catalog_reports_its_language_escapes_once_and_keeps_the_csp() {
    let fixture = Fixture::new().await;
    for locale in LOCALES {
        for (path, hostile) in pages() {
            let context = format!("{path} in {}", locale.tag());
            let page = fixture.get(&path, &[("Accept-Language", locale.tag())]).await;
            assert_eq!(page.status, StatusCode::OK, "{context}");
            if Locale::LANGUAGES.contains(&locale) {
                assert_eq!(page.header("content-language"), locale.tag(), "{context}: the negotiated language");
            }
            assert_language_matches_body(&page, &context);
            assert_strict_csp(&page, &context);
            if hostile {
                assert_escaped_once(&page, &context);
            }
        }
        let missing = fixture.get("/safety-owner/demo/no-such-page", &[("Accept-Language", locale.tag())]).await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);
        assert_language_matches_body(&missing, &format!("404 in {}", locale.tag()));
        let dashboard = fixture.get("/", &[("Accept-Language", locale.tag()), ("Cookie", &fixture.cookie)]).await;
        assert_language_matches_body(&dashboard, &format!("dashboard in {}", locale.tag()));
        assert_strict_csp(&dashboard, &format!("dashboard in {}", locale.tag()));
    }
}

#[tokio::test]
async fn a_validator_from_one_catalog_never_revalidates_another() {
    let fixture = Fixture::new().await;
    let path = "/safety-owner/demo/issues/1";
    let mut tags = Vec::new();
    for locale in Locale::LANGUAGES {
        let page = fixture.get(path, &[("Accept-Language", locale.tag())]).await;
        tags.push((locale, page.header("etag").to_owned()));
    }
    for (index, (locale, tag)) in tags.iter().enumerate() {
        assert!(tags[..index].iter().all(|(_, other)| other != tag), "{}: validators are distinct per catalog", locale.tag());
        for (other, _) in &tags {
            let page = fixture.get(path, &[("Accept-Language", other.tag()), ("If-None-Match", tag)]).await;
            let expected = if other == locale { StatusCode::NOT_MODIFIED } else { StatusCode::OK };
            assert_eq!(page.status, expected, "{}'s validator sent in {}", locale.tag(), other.tag());
            assert_eq!(page.header("rbit-catalog-key"), other.catalog_key(), "the answer names the requested catalog");
            if expected == StatusCode::OK {
                assert_eq!(page.attribute("lang"), Some(other.tag()), "the body is the requested catalog's");
            }
        }
    }
}
