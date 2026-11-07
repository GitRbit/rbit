//! Conditional requests never stand in for authorization: a validator obtained by someone
//! who may read a private repository earns nobody else a `304`, not even its holder once
//! access is revoked, and private pages are only ever cacheable by the viewer's browser.

use axum::http::{HeaderValue, Method, StatusCode, header};

use super::{Credential, Fixture};
use crate::permission_world::{ACTORS, Actor, SHAPES, Shape, effective};

fn case(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

async fn conditional(fixture: &Fixture, path: &str, actor: Actor, validator: &str) -> super::Answer {
    let request = fixture.request(Method::GET, path, actor, Credential::Session).header(header::IF_NONE_MATCH, validator);
    fixture.send(request, axum::body::Body::empty()).await
}

#[tokio::test]
async fn validators_of_private_pages_do_not_leak_to_other_viewers() {
    let fixture = Fixture::new().await;
    for shape in SHAPES.into_iter().filter(|shape| !shape.public) {
        for page in [format!("/{}", shape.full_name()), format!("/{}/commits", shape.full_name())] {
            for reader in ACTORS.into_iter().filter(|actor| effective(*actor, shape).can_read()) {
                let fresh = fixture.get(&page, reader, Credential::Session).await;
                assert_eq!(fresh.status, StatusCode::OK, "{page}: {}", case(reader, shape));
                let cache = fresh.headers[header::CACHE_CONTROL].to_str().unwrap();
                assert!(cache.contains("private") && !cache.contains("public"), "{page}: {cache}");
                let etag = fresh.headers[header::ETAG].to_str().unwrap().to_owned();
                let again = conditional(&fixture, &page, reader, &etag).await;
                assert_eq!(again.status, StatusCode::NOT_MODIFIED, "the validator is real: {page} {}", case(reader, shape));
                for other in ACTORS.into_iter().filter(|actor| !effective(*actor, shape).can_read()) {
                    for validator in [etag.as_str(), "*"] {
                        let answer = conditional(&fixture, &page, other, validator).await;
                        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{page} {validator}: {}", case(other, shape));
                        assert!(!answer.headers.contains_key(header::ETAG), "{page}: {}", case(other, shape));
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn revoking_access_invalidates_a_held_validator() {
    let fixture = Fixture::new().await;
    let shape = Shape { organization: false, public: false, archived: false };
    let page = format!("/{}", shape.full_name());
    let reader = fixture.world.user(Actor::CollaboratorRead).unwrap().id();
    let fresh = fixture.get(&page, Actor::CollaboratorRead, Credential::Session).await;
    let etag = fresh.headers[header::ETAG].clone();
    assert_eq!(conditional(&fixture, &page, Actor::CollaboratorRead, etag.to_str().unwrap()).await.status, StatusCode::NOT_MODIFIED);
    fixture.database.collaborators().remove(fixture.world.repo(shape).id, reader).await.unwrap();
    let answer = conditional(&fixture, &page, Actor::CollaboratorRead, etag.to_str().unwrap()).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "access is decided before the validator");
    assert_ne!(answer.headers.get(header::ETAG), Some(&etag));
}

/// Archive downloads carry the commit as their validator; it is checked after access.
#[tokio::test]
async fn archive_validators_follow_the_oracle() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        let path = format!("/{}/archive/main.tar.gz", shape.full_name());
        let validator = format!("\"{}\"", fixture.world.repo(shape).head);
        for actor in ACTORS {
            let answer = conditional(&fixture, &path, actor, &validator).await;
            let expected = if effective(actor, shape).can_read() { StatusCode::NOT_MODIFIED } else { StatusCode::NOT_FOUND };
            assert_eq!(answer.status, expected, "{}", case(actor, shape));
            if !shape.public && expected == StatusCode::NOT_MODIFIED {
                let cache = answer.headers.get(header::CACHE_CONTROL).map(HeaderValue::to_str).unwrap().unwrap();
                assert!(cache.starts_with("private"), "{cache}");
            }
        }
    }
}
