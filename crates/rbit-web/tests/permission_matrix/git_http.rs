//! Git smart HTTP ingress: fetch and push advertisements and a real `receive-pack` exchange
//! for every actor's token (as Git sends it, in `Basic`), archived repositories included.

use axum::http::{Method, StatusCode, header};
use rbit_core::RepoAccess;

use super::{Credential, Fixture, PROBE};
use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, effective, git};

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

/// What Git is answered before anything runs: a credential prompt for anonymous callers
/// and refused credentials, 404 for authenticated callers who may not read.
fn denied(actor: Actor, access: RepoAccess, needs_write: bool) -> Option<StatusCode> {
    let refused = match actor {
        // A presented credential that was refused prompts again even for public repositories.
        Actor::Suspended => true,
        Actor::Anonymous => !access.can_read() || needs_write,
        _ => {
            return (!access.can_read())
                .then_some(StatusCode::NOT_FOUND)
                .or((needs_write && !access.can_write()).then_some(StatusCode::FORBIDDEN));
        }
    };
    refused.then_some(StatusCode::UNAUTHORIZED)
}

#[tokio::test]
async fn advertisements_follow_the_oracle() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        for actor in ACTORS {
            let access = effective(actor, shape);
            for (service, needs_write) in [("git-upload-pack", false), ("git-receive-pack", true)] {
                let path = format!("/{}.git/info/refs?service={service}", shape.full_name());
                let answer = fixture.get(&path, actor, Credential::Basic).await;
                let expected = denied(actor, access, needs_write).unwrap_or(StatusCode::OK);
                assert_eq!(answer.status, expected, "{service}: {}", case(actor, shape));
                if answer.status == StatusCode::UNAUTHORIZED {
                    assert!(answer.headers.contains_key(header::WWW_AUTHENTICATE), "{service}: {}", case(actor, shape));
                }
            }
        }
    }
}

/// Deleting a branch needs no pack, so the push gate decides alone: writers delete their
/// probe branch, archived repositories refuse every update, nobody else reaches git.
#[tokio::test]
async fn pushes_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let repo = fixture.world.repo(shape);
        for actor in ACTORS {
            let access = effective(actor, shape);
            let branch = format!("refs/heads/{PROBE}-{}", actor.login().unwrap_or("nobody"));
            let command = format!("{} {} {branch}\0report-status\n", repo.head, "0".repeat(40));
            let mut body = rbit_transport::pkt_line::encode(&command);
            body.extend_from_slice(b"0000");
            let request = fixture
                .request(Method::POST, &format!("/{}.git/git-receive-pack", shape.full_name()), actor, Credential::Basic)
                .header(header::CONTENT_TYPE, "application/x-git-receive-pack-request");
            let answer = fixture.send(request, body).await;
            let exists = || git(&repo.disk_path, &["for-each-ref", &branch]).contains(&branch);
            if let Some(expected) = denied(actor, access, true) {
                assert_eq!(answer.status, expected, "{}", case(actor, shape));
                assert!(actor.login().is_none() || exists(), "untouched: {}", case(actor, shape));
                continue;
            }
            assert_eq!(answer.status, StatusCode::OK, "{}: {}", case(actor, shape), answer.text());
            let report = answer.text();
            if shape.archived {
                assert!(report.contains(&format!("ng {branch} ")), "{}: {report}", case(actor, shape));
                assert!(report.contains("archived"), "{}: {report}", case(actor, shape));
                assert!(exists(), "archived refs stay: {}", case(actor, shape));
            } else {
                assert!(report.contains(&format!("ok {branch}")), "{}: {report}", case(actor, shape));
                assert!(!exists(), "deleted: {}", case(actor, shape));
            }
        }
    }
}
