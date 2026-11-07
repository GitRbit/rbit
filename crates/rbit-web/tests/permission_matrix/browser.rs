//! Browser ingress: repository pages, content forms, browser edits, settings and the
//! listings a browser shows, for every actor's session cookie.

use axum::http::StatusCode;
use rbit_core::RepoAccess;

use super::{Answer, Credential, Fixture, asks_to_sign_in};
use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, effective, git};

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

/// What a signed-in-only form answers: the sign-in page for anyone without a live session
/// (anonymous visitors and the suspended account), 404 for repositories they cannot read.
fn gate(actor: Actor, access: RepoAccess, answer: &Answer) -> bool {
    if !actor.authenticates() {
        assert!(asks_to_sign_in(answer), "{actor:?}: {} {:?}", answer.status, answer.location());
        return false;
    }
    if !access.can_read() {
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{actor:?}: {}", answer.text());
        return false;
    }
    true
}

#[tokio::test]
async fn pages_and_settings_follow_the_oracle() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let home = format!("/{}", shape.full_name());
        for actor in ACTORS {
            let access = effective(actor, shape);
            let page = fixture.get(&home, actor, Credential::Session).await;
            let expected = if access.can_read() { StatusCode::OK } else { StatusCode::NOT_FOUND };
            assert_eq!(page.status, expected, "home: {}", case(actor, shape));
            let settings = fixture.get(&format!("{home}/settings"), actor, Credential::Session).await;
            // Settings stay available to administrators of archived repositories.
            let expected = if access.can_admin() { StatusCode::OK } else { StatusCode::NOT_FOUND };
            assert_eq!(settings.status, expected, "settings: {}", case(actor, shape));
        }
    }
}

#[tokio::test]
async fn issue_and_label_forms_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let home = format!("/{}", shape.full_name());
        for actor in ACTORS {
            let access = effective(actor, shape);
            let issue = fixture.post_form(&format!("{home}/issues"), actor, &format!("title={actor:?}+report")).await;
            if gate(actor, access, &issue) {
                if shape.archived {
                    assert_eq!(issue.status, StatusCode::BAD_REQUEST, "issue: {}", case(actor, shape));
                } else {
                    assert!(issue.status.is_redirection(), "issue: {}: {}", case(actor, shape), issue.status);
                    assert!(issue.location().unwrap().starts_with(&format!("{home}/issues/")), "issue: {}", case(actor, shape));
                }
            }
            let label = fixture.post_form(&format!("{home}/labels"), actor, &format!("name={actor:?}&color=ededed")).await;
            if gate(actor, access, &label) {
                let expected = match (access.can_write(), shape.archived) {
                    (false, _) => StatusCode::FORBIDDEN,
                    (true, true) => StatusCode::BAD_REQUEST,
                    (true, false) => StatusCode::SEE_OTHER,
                };
                assert_eq!(label.status, expected, "label: {}: {}", case(actor, shape), label.text());
            }
        }
    }
}

/// Browser edits answer 404 to everyone who may not change the content (readers never
/// learn the pages exist), archived repositories included, and commit for writers.
#[tokio::test]
async fn browser_edits_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let home = format!("/{}", shape.full_name());
        let disk = fixture.world.repo(shape).disk_path.clone();
        for actor in ACTORS {
            let access = effective(actor, shape);
            let writable = access.can_write() && !shape.archived;
            let form = fixture.get(&format!("{home}/new/main"), actor, Credential::Session).await;
            if gate(actor, access, &form) {
                let expected = if writable { StatusCode::OK } else { StatusCode::NOT_FOUND };
                assert_eq!(form.status, expected, "edit form: {}", case(actor, shape));
            }
            let tip = git(&disk, &["rev-parse", "main"]);
            let file = format!("edits/{}.txt", actor.login().unwrap_or("anonymous"));
            let body = format!("path={file}&content=hello&commit={tip}&title=Add+{actor:?}&target=branch");
            let commit = fixture.post_form(&format!("{home}/new/main"), actor, &body).await;
            if gate(actor, access, &commit) {
                let expected = if writable { StatusCode::SEE_OTHER } else { StatusCode::NOT_FOUND };
                assert_eq!(commit.status, expected, "edit commit: {}: {}", case(actor, shape), commit.text());
            }
            let moved = git(&disk, &["rev-parse", "main"]) != tip;
            assert_eq!(moved, writable && actor.authenticates(), "branch moved: {}", case(actor, shape));
        }
    }
}

/// Owner pages and explore list exactly the repositories the viewer may read.
#[tokio::test]
async fn browser_listings_omit_unreadable_repositories() {
    let fixture = Fixture::new().await;
    for actor in ACTORS {
        for owner in ["alice", "acme"] {
            let page = fixture.get(&format!("/{owner}"), actor, Credential::Session).await;
            assert_eq!(page.status, StatusCode::OK, "{actor:?} /{owner}");
            let text = page.text();
            for shape in SHAPES.into_iter().filter(|shape| shape.owner() == owner) {
                let linked = text.contains(&format!("\"/{}\"", shape.full_name()));
                assert_eq!(linked, effective(actor, shape).can_read(), "{actor:?} sees {} on /{owner}", shape.full_name());
            }
        }
        let explore = fixture.get("/explore", actor, Credential::Session).await.text();
        for shape in SHAPES {
            assert_eq!(explore.contains(&format!("\"/{}\"", shape.full_name())), shape.public, "{actor:?} explores {}", shape.full_name());
        }
    }
}
