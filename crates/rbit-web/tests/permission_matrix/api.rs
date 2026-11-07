//! REST API ingress: reads, content writes, administration and listings for every
//! actor's personal access token (`admin` scope, so only the repository decides).

use std::collections::BTreeSet;

use axum::http::{Method, StatusCode};
use rbit_core::RepoAccess;
use serde_json::{Value, json};

use super::{Answer, Fixture};
use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, effective};

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
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

fn check(answer: &Answer, expected: StatusCode, what: &str) {
    assert_eq!(answer.status, expected, "{what}: {}", answer.text());
}

#[tokio::test]
async fn api_repository_endpoints_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let repo = format!("/repos/{}", shape.full_name());
        for actor in ACTORS {
            let access = effective(actor, shape);
            let what = case(actor, shape);
            let anonymous = actor == Actor::Anonymous;

            let read = fixture.api(Method::GET, &repo, actor, None).await;
            check(&read, refused(actor, access).unwrap_or(StatusCode::OK), &format!("read {what}"));

            let issue = fixture.api(Method::POST, &format!("{repo}/issues"), actor, Some(json!({"title": "report"}))).await;
            let expected = refused(actor, access).unwrap_or(match (anonymous, shape.archived) {
                (true, _) => StatusCode::UNAUTHORIZED,
                (false, true) => StatusCode::FORBIDDEN,
                (false, false) => StatusCode::CREATED,
            });
            check(&issue, expected, &format!("issue {what}"));

            let label = json!({"name": format!("{actor:?}"), "color": "ededed"});
            let label = fixture.api(Method::POST, &format!("{repo}/labels"), actor, Some(label)).await;
            let expected = refused(actor, access).unwrap_or(match (anonymous, access.can_write(), shape.archived) {
                (true, ..) => StatusCode::UNAUTHORIZED,
                (false, false, _) | (false, true, true) => StatusCode::FORBIDDEN,
                (false, true, false) => StatusCode::CREATED,
            });
            check(&label, expected, &format!("label {what}"));

            let status = json!({"state": "success", "context": format!("{actor:?}")});
            let head = &fixture.world.repo(shape).head;
            let status = fixture.api(Method::POST, &format!("{repo}/statuses/{head}"), actor, Some(status)).await;
            check(&status, expected, &format!("status {what}"));

            // Settings stay available to administrators of archived repositories.
            let update = fixture.api(Method::PATCH, &repo, actor, Some(json!({}))).await;
            let expected = refused(actor, access).unwrap_or(match (anonymous, access.can_admin()) {
                (true, _) => StatusCode::UNAUTHORIZED,
                (false, false) => StatusCode::FORBIDDEN,
                (false, true) => StatusCode::OK,
            });
            check(&update, expected, &format!("update {what}"));
        }
    }
}

fn names(answer: &Answer) -> BTreeSet<String> {
    let json: Value = serde_json::from_slice(&answer.body).unwrap();
    json.as_array().unwrap().iter().map(|repo| repo["full_name"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn api_listings_omit_unreadable_repositories() {
    let fixture = Fixture::new().await;
    for actor in ACTORS {
        for (owner, path) in [("alice", "/users/alice/repos?per_page=100"), ("acme", "/orgs/acme/repos?per_page=100")] {
            let answer = fixture.api(Method::GET, path, actor, None).await;
            if actor == Actor::Suspended {
                check(&answer, StatusCode::UNAUTHORIZED, &format!("{actor:?} {path}"));
                continue;
            }
            check(&answer, StatusCode::OK, &format!("{actor:?} {path}"));
            let readable: BTreeSet<String> = SHAPES
                .into_iter()
                .filter(|shape| shape.owner() == owner && effective(actor, *shape).can_read())
                .map(Shape::full_name)
                .collect();
            assert_eq!(names(&answer), readable, "{actor:?} {path}");
        }
    }
}
