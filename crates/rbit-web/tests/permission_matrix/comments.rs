//! Comment creation on the seeded issue of every repository, through the REST API and the
//! browser form, for every actor. A comment needs read access and a signed-in account;
//! an archived repository refuses every comment, and a locked thread would need write
//! access (`threads::permissions::issue_permissions`). Both ingresses must agree.

use axum::http::{Method, StatusCode};
use rbit_core::RepoAccess;
use serde_json::json;

use super::{Answer, Fixture};
use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, effective};

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

fn check(answer: &Answer, expected: StatusCode, what: &str) {
    assert_eq!(answer.status, expected, "{what}: {}", answer.text());
}

/// The status every API request answers before its own rule: 401 for a refused token,
/// 404 for a repository the caller may not read. `None` when the endpoint decides.
fn refused(actor: Actor, access: RepoAccess) -> Option<StatusCode> {
    if actor == Actor::Suspended {
        Some(StatusCode::UNAUTHORIZED)
    } else if !access.can_read() {
        Some(StatusCode::NOT_FOUND)
    } else {
        None
    }
}

/// What a signed-in-only form answers: the sign-in page for anyone without a live session,
/// 404 for repositories they cannot read.
fn gate(actor: Actor, access: RepoAccess, answer: &Answer) -> bool {
    if !actor.authenticates() {
        assert!(answer.status.is_redirection(), "{actor:?}: {}", answer.status);
        return false;
    }
    if !access.can_read() {
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{actor:?}: {}", answer.text());
        return false;
    }
    true
}

#[tokio::test]
async fn api_comments_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let repo = format!("/repos/{}", shape.full_name());
        let issue = fixture.world.repo(shape).issue;
        for actor in ACTORS {
            let access = effective(actor, shape);
            let what = case(actor, shape);
            let anonymous = actor == Actor::Anonymous;
            let body = json!({ "body": format!("{actor:?} says hi") });
            let comment = fixture.api(Method::POST, &format!("{repo}/issues/{issue}/comments"), actor, Some(body)).await;
            let expected = refused(actor, access).unwrap_or(match (anonymous, shape.archived) {
                (true, _) => StatusCode::UNAUTHORIZED,
                (false, true) => StatusCode::FORBIDDEN,
                (false, false) => StatusCode::CREATED,
            });
            check(&comment, expected, &format!("comment {what}"));
        }
    }
}

#[tokio::test]
async fn browser_comments_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let home = format!("/{}", shape.full_name());
        let issue = fixture.world.repo(shape).issue;
        for actor in ACTORS {
            let access = effective(actor, shape);
            let form = format!("body={actor:?}+says+hi");
            let comment = fixture.post_form(&format!("{home}/issues/{issue}/comments"), actor, &form).await;
            if gate(actor, access, &comment) {
                if shape.archived {
                    assert_eq!(comment.status, StatusCode::BAD_REQUEST, "comment: {}: {}", case(actor, shape), comment.text());
                } else {
                    assert!(comment.status.is_redirection(), "comment: {}: {}", case(actor, shape), comment.status);
                }
            }
        }
    }
}
