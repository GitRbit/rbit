//! Real Code-to-Commits links retain selected refs without changing repository scope.
use super::{Fixture, HOME, StatusCode, git};

fn history_link(html: &str) -> &str {
    let anchor = html.split("data-repo-tab=\"commits\"").nth(1).expect("Commits tab").split('>').next().unwrap();
    anchor.split("href=\"").nth(1).unwrap().split('"').next().unwrap()
}

#[tokio::test]
async fn code_history_navigation_preserves_slash_refs_tags_and_exact_commit_ids() {
    let fixture = Fixture::new().await;
    git(&fixture.bare, &["update-ref", "refs/heads/feature/nav%check", &fixture.branch_commit]);
    git(&fixture.bare, &["update-ref", "refs/tags/release/nav", &fixture.main_commit]);
    for (revision, encoded, branch) in [
        ("docs-next", "docs-next", true),
        ("feature/nav%check", "feature/nav%25check", true),
        ("release/nav", "release/nav", false),
        (fixture.branch_commit.as_str(), fixture.branch_commit.as_str(), true),
    ] {
        for (kind, suffix) in [("tree", ""), ("tree", "/docs"), ("blob", "/README.md"), ("blame", "/README.md")] {
            let (status, code) = fixture.get(&format!("{HOME}/{kind}/{encoded}{suffix}"), false, false).await;
            assert_eq!(status, StatusCode::OK, "{revision} {kind}");
            assert!(code.contains(&format!("data-repo-scope=\"{HOME}\"")), "scope is not a selected tree URL");
            let expected = format!("{HOME}/commits/{encoded}");
            assert_eq!(history_link(&code), expected);
            // Follow the actual rendered no-JS link, not a reconstructed route.
            let (status, history) = fixture.get(history_link(&code), false, false).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(history_link(&history), expected);
            assert_eq!(history.contains("Branch documentation"), branch);
        }
    }
}

#[tokio::test]
async fn default_and_empty_repositories_keep_canonical_unselected_history_links() {
    let fixture = Fixture::new().await;
    for path in [HOME.to_owned(), format!("{HOME}/tree/main"), "/docs-owner/empty".to_owned()] {
        let (status, html) = fixture.get(&path, false, false).await;
        assert_eq!(status, StatusCode::OK);
        let base = if path.contains("/empty") { "/docs-owner/empty" } else { HOME };
        assert_eq!(history_link(&html), format!("{base}/commits"));
        let (status, commits) = fixture.get(history_link(&html), false, false).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(history_link(&commits), format!("{base}/commits"));
    }
}
