//! Wiki pages through the real router: reads, forms, conflicts, permissions and locale.
mod changes;
mod fixture;

use axum::http::StatusCode;
use fixture::Fixture;
use rbit_core::Visibility;

const W: &str = "/owner/handbook/wiki";

#[tokio::test]
async fn public_pages_render_sanitized_markdown_for_anonymous_readers_with_the_tab() {
    let fixture = Fixture::new(Visibility::Public).await;
    fixture.page("Home", "# Welcome\n\n**bold** <script>alert(1)</script><img src=x onerror=alert(1)>").await;
    fixture.page("Setup guide", "steps").await;
    let home = fixture.get(W, None).await;
    assert_eq!(home.status, StatusCode::OK);
    for expected in
        ["<strong>bold</strong>", "data-repo-tab=\"wiki\"", "aria-current=\"page\">Wiki</a>", "/owner/handbook/wiki/Setup-guide"]
    {
        assert!(home.body.contains(expected), "missing {expected}");
    }
    for forbidden in ["<script>alert", "onerror"] {
        assert!(!home.body.contains(forbidden));
    }
    assert!(!home.body.contains("/wiki/-/new"), "readers get no write controls");
    // Static `W/-/…` routes win over the page catch-all; `-` is never a page name.
    assert_eq!(fixture.get(&format!("{W}/-/pages"), None).await.status, StatusCode::OK);
    let page = fixture.get(&format!("{W}/Setup-guide"), None).await;
    assert!(page.body.contains("Setup guide") && page.body.contains("steps"));
    let found = fixture.get(&format!("{W}/-/search?q=setup"), None).await;
    assert!(found.body.contains("1 page matches") && found.body.contains("Setup-guide"));
    assert_eq!(fixture.get(&format!("{W}/Missing"), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(fixture.get(&format!("{W}/a/../b"), None).await.status, StatusCode::NOT_FOUND);
    let repository = fixture.get("/owner/handbook", None).await;
    assert!(repository.body.contains("data-repo-tab=\"wiki\""));
}

#[tokio::test]
async fn private_wikis_are_hidden_and_changes_need_write_access() {
    let fixture = Fixture::new(Visibility::Private).await;
    fixture.page("Home", "private").await;
    for user in [None, Some(&fixture.outsider)] {
        assert_eq!(fixture.get(W, user).await.status, StatusCode::NOT_FOUND);
        assert_eq!(fixture.get(&format!("{W}/-/pages"), user).await.status, StatusCode::NOT_FOUND);
    }
    assert_eq!(fixture.get(W, Some(&fixture.reader)).await.status, StatusCode::OK);
    let form = [("title", "New"), ("path", ""), ("content", "x"), ("intent", "save")];
    let anonymous = fixture.post(&format!("{W}/-/new"), None, &form).await;
    assert_eq!(anonymous.status, StatusCode::SEE_OTHER);
    assert!(anonymous.location.unwrap().starts_with("/login?next="));
    assert_eq!(fixture.post(&format!("{W}/-/new"), Some(&fixture.outsider), &form).await.status, StatusCode::NOT_FOUND);
    assert_eq!(fixture.post(&format!("{W}/-/new"), Some(&fixture.reader), &form).await.status, StatusCode::FORBIDDEN);
    assert_eq!(fixture.get(&format!("{W}/-/new"), Some(&fixture.reader)).await.status, StatusCode::FORBIDDEN);
    let created = fixture.post(&format!("{W}/-/new"), Some(&fixture.writer), &form).await;
    assert_eq!((created.status, created.location.as_deref()), (StatusCode::SEE_OTHER, Some("/owner/handbook/wiki/New")));
    assert!(fixture.get(&format!("{W}/New"), Some(&fixture.reader)).await.body.contains("Last edited"));

    fixture.archive().await;
    let archived = fixture.post(&format!("{W}/-/new"), Some(&fixture.writer), &[("title", "Late"), ("content", "x")]).await;
    assert_eq!(archived.status, StatusCode::BAD_REQUEST);
    let read = fixture.get(&format!("{W}/New"), Some(&fixture.writer)).await;
    assert_eq!(read.status, StatusCode::OK);
    assert!(!read.body.contains("/edit\""), "archived wikis show no edit controls");
}

#[tokio::test]
async fn a_turned_off_wiki_is_missing_except_for_administrators() {
    let fixture = Fixture::new(Visibility::Public).await;
    let off = [("enabled", "off")];
    assert_eq!(fixture.post(&format!("{W}/-/settings"), Some(&fixture.writer), &off).await.status, StatusCode::FORBIDDEN);
    assert_eq!(fixture.post(&format!("{W}/-/settings"), Some(&fixture.owner), &off).await.status, StatusCode::SEE_OTHER);
    assert_eq!(fixture.get(W, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(fixture.get(W, Some(&fixture.writer)).await.status, StatusCode::NOT_FOUND);
    assert!(!fixture.get("/owner/handbook", Some(&fixture.writer)).await.body.contains("data-repo-tab=\"wiki\""));
    let admin = fixture.get(W, Some(&fixture.owner)).await;
    assert_eq!(admin.status, StatusCode::OK);
    assert!(admin.body.contains("Turn on the wiki"));
    assert!(fixture.get("/owner/handbook", Some(&fixture.owner)).await.body.contains("data-repo-tab=\"wiki\""));
    let on = fixture.post(&format!("{W}/-/settings"), Some(&fixture.owner), &[("enabled", "on")]).await;
    assert_eq!(on.location.as_deref(), Some(W));
    let empty = fixture.get(W, Some(&fixture.writer)).await;
    assert!(empty.body.contains("This wiki has no pages yet.") && empty.body.contains("Create the first page"));
}

#[tokio::test]
async fn pages_render_in_the_rtl_diagnostic_locale_with_isolated_user_text() {
    let fixture = Fixture::new(Visibility::Public).await;
    fixture.page("Home", "text").await;
    let page = fixture.get_with(&format!("{W}/-/pages"), None, &[("accept-language", "ar-XB")]).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("dir=\"rtl\"") && page.body.contains("lang=\"ar-XB\""));
    assert!(page.body.contains("[All pages]"), "wiki text comes from the catalog");
    let history = fixture.get_with(&format!("{W}/-/search?q=Ho"), None, &[("accept-language", "ar-XB")]).await;
    assert!(history.body.contains("\u{2068}Ho\u{2069}"), "interpolated user text is isolated");
}
