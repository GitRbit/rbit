//! Themes: the gallery, choosing a theme with and without JavaScript, and a signed-in
//! user's theme following them to a new device.

use rbit_e2e::{Client, PASSWORD, StatusCode, TestServer};

const DRACULA: &str = "base24-dracula";

/// The `data-theme` the server renders on `<html>` (None for System).
async fn rendered_theme(client: &Client) -> Option<String> {
    let page = client.get("/explore").await;
    page.expect_status(StatusCode::OK);
    let start = page.body.find("<html")?;
    let tag = &page.body[start..start + page.body[start..].find('>')?];
    let value = tag.split("data-theme=\"").nth(1)?;
    Some(value[..value.find('"')?].to_owned())
}

#[tokio::test]
async fn anonymous_visitors_choose_a_theme_without_javascript() {
    let server = TestServer::start();
    let visitor = server.client();

    let gallery = visitor.get("/themes").await;
    gallery.expect_status(StatusCode::OK).expect_contains(&[
        "data-theme-gallery",
        "/assets/themes/all.css?v=",
        &format!("data-theme=\"{DRACULA}\" value=\"{DRACULA}\""),
        "<form method=\"post\" action=\"/themes\"",
    ]);
    assert_eq!(rendered_theme(&visitor).await, None, "System by default");

    let chosen = visitor.post("/themes", &[("theme", DRACULA), ("next", "/explore")]).await;
    chosen.expect_redirect("/explore");
    let cookie = chosen.header("set-cookie").expect("the choice is remembered");
    assert!(cookie.contains(&format!("rbit-theme={DRACULA}")) && cookie.contains("Max-Age=31536000"), "{cookie}");
    assert_eq!(rendered_theme(&visitor).await.as_deref(), Some(DRACULA));
    visitor.get("/explore").await.expect_contains(&[&format!("/assets/themes/{DRACULA}.css?v="), "data-scheme=\"dark\""]);

    // The picker script gets an empty answer; unknown themes and open redirects are refused.
    visitor.post("/themes", &[("theme", "light"), ("js", "1")]).await.expect_status(StatusCode::NO_CONTENT);
    assert_eq!(rendered_theme(&visitor).await.as_deref(), Some("light"));
    visitor.post("/themes", &[("theme", "base16-nope")]).await.expect_status(StatusCode::BAD_REQUEST);
    visitor.post("/themes", &[("theme", "dark"), ("next", "//evil.example")]).await.expect_redirect("/");

    // Filters work server-side too.
    let dark = visitor.get("/themes?variant=dark&q=dracula").await;
    dark.expect_status(StatusCode::OK).expect_contains(&["aria-current=\"true\">Dark", "value=\"dracula\""]);
}

#[tokio::test]
async fn a_signed_in_users_theme_follows_them_to_new_devices() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.get("/settings/appearance").await.expect_status(StatusCode::OK).expect_contains(&["Appearance", "data-theme-gallery"]);
    alice.post("/themes", &[("theme", DRACULA), ("next", "/settings/appearance")]).await.expect_redirect("/settings/appearance");

    // A fresh browser has no cookie until alice signs in there.
    let laptop = server.client();
    assert_eq!(rendered_theme(&laptop).await, None);
    laptop.sign_in("alice", PASSWORD).await.expect_redirect("/");
    assert_eq!(rendered_theme(&laptop).await.as_deref(), Some(DRACULA));

    // Anonymous visitors cannot open the settings page.
    let anonymous = server.client().get("/settings/appearance").await;
    anonymous.expect_status(StatusCode::SEE_OTHER);
}
