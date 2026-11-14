use super::*;

#[tokio::test]
async fn file_browser_uses_the_same_pinned_listing_with_or_without_language_and_query() {
    let fixture = Fixture::new().await;
    let browser = format!("{HOME}/language-files?revision={}", fixture.main_commit);
    let rust = format!("{HOME}/blob/{}/main.rs", fixture.main_commit);
    let text = format!("{HOME}/blob/{}/secret.txt", fixture.main_commit);

    let (status, unfiltered) = fixture.get(&browser, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(unfiltered.contains(&rust));
    assert!(unfiltered.contains(&text));

    let (status, filtered) = fixture.get(&format!("{browser}&language=Rust&q=main"), false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(filtered.contains(&rust));
    assert!(!filtered.contains(&text));
    assert!(filtered.contains(&format!("href=\"{browser}\"")), "expected a link that clears both filters and keeps the revision");

    let (status, query_only) = fixture.get(&format!("{browser}&q=secret"), false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(query_only.contains(&text));
    assert!(!query_only.contains(&rust));

    let (status, fuzzy) = fixture.get(&format!("{browser}&q=mainrs"), false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fuzzy.contains(&rust));

    let legacy = format!("{HOME}/find/{}?q=secret", fixture.main_commit);
    let (status, legacy) = fixture.get(&legacy, false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(legacy.contains(&text));
}

#[tokio::test]
async fn the_template_form_can_create_a_repository_from_the_source_page() {
    let fixture = Fixture::new().await;
    let (status, source) = fixture.get(HOME, true, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(source.contains("/new?template=docs-owner/public"));

    let (status, form) = fixture.get("/new?template=docs-owner/public", true, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(form.contains("name=\"template\" value=\"docs-owner/public\""));
    let (status, redirect) =
        fixture.submit("/new", "owner=docs-owner&name=from-template&template=docs-owner%2Fpublic&visibility=public".into()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(redirect.as_deref(), Some("/docs-owner/from-template"));
    let (status, created) = fixture.get(&redirect.unwrap(), false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(created.contains("Root documentation marker"));
    assert!(!created.contains("Nondefault branch marker"));
}

#[tokio::test]
async fn the_fork_page_can_fast_forward_its_default_branch_to_upstream() {
    let fixture = Fixture::new().await;
    let (status, redirect) = fixture.submit(&format!("{HOME}/fork"), "owner=docs-owner&name=working-fork".into()).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let fork = redirect.unwrap();

    git(&fixture.source, &["checkout", "-q", "main"]);
    std::fs::write(fixture.source.join("README.md"), "# Upstream sync marker\n").unwrap();
    git(&fixture.source, &["commit", "-qam", "Advance upstream"]);
    let upstream_tip = git(&fixture.source, &["rev-parse", "HEAD"]);
    git(&fixture.bare, &["fetch", "-q", fixture.source.to_str().unwrap(), "main:main"]);

    let sync = format!("{fork}/fork/sync");
    let (status, home) = fixture.get(&fork, true, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(home.contains(&sync));
    let (status, panel) = fixture.get(&sync, true, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(panel.contains("name=\"upstream_tip\""));
    assert!(panel.contains(&upstream_tip));

    let input = format!("branch=main&fork_tip={}&upstream_branch=main&upstream_tip={upstream_tip}", fixture.main_commit);
    let (status, redirect) = fixture.submit(&sync, input).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(redirect.as_deref(), Some(fork.as_str()));
    let (status, updated) = fixture.get(&fork, true, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(updated.contains("Upstream sync marker"));
}
