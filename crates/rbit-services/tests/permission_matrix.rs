//! Mandatory permission regression group, service boundary: the shared oracle is the pure
//! policy, and every service entry point an ingress uses to open, list or change a
//! repository decides exactly what that oracle says, archived repositories included.

use std::{collections::BTreeSet, sync::Arc};

use rbit_auth::{Grants, repo_access};
use rbit_core::{Config, RepoAccess, RepoSort, User, repository::ArchivedFilter};
use rbit_services::{
    CreateAccessToken, RepoContext, RepoSearch, ServiceError, Services,
    issues::{LabelInput, NewIssueInput},
};
use rbit_storage::{Database, KvStore};

use crate::permission_world::{ACTORS, Actor, Relation, SHAPES, Shape, World, oracle, relation};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    world: World,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(config, &database, &kv).await.unwrap();
        let world = World::build(&services, &database, dir.path(), &[]).await;
        Self { _dir: dir, database, services, world }
    }

    /// `open` as the ingresses report it: the access, or `None` for "not found".
    fn opened(result: Result<RepoContext, ServiceError>) -> (RepoAccess, Option<RepoContext>) {
        match result {
            Ok(ctx) => (ctx.access, Some(ctx)),
            Err(ServiceError::NotFound) => (RepoAccess::None, None),
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}

fn grants(relation: Relation) -> Grants {
    Grants { org_role: relation.org_role, org_base: relation.org_base, team: relation.team, collaborator: relation.collaborator }
}

fn label(actor: Actor, shape: Shape) -> String {
    format!("{actor:?} on {}", shape.full_name())
}

#[tokio::test]
async fn the_oracle_is_the_pure_policy_for_every_declared_relation() {
    let fixture = Fixture::new().await;
    let mut checked = 0;
    for shape in SHAPES {
        let repo = fixture.database.repositories().find_by_path(shape.owner(), shape.name()).await.unwrap().unwrap();
        assert_eq!(repo.repo.is_archived(), shape.archived, "{}", shape.full_name());
        for actor in ACTORS {
            let viewer = fixture.world.user(actor);
            let declared = relation(actor, shape);
            if let (Some(user), Some(declared)) = (viewer, declared) {
                // The entity facts the policy adapter derives must match what was declared.
                assert_eq!(user.is_admin, declared.site_admin, "{}", label(actor, shape));
                assert_eq!(user.id() == repo.repo.owner_id, declared.repository_owner, "{}", label(actor, shape));
            }
            let policy = repo_access(viewer, &repo, &declared.map(grants).unwrap_or_default());
            assert_eq!(policy, oracle(actor, shape), "{}", label(actor, shape));
            checked += 1;
        }
    }
    assert_eq!(checked, ACTORS.len() * SHAPES.len());
}

/// What the store actually granted is what the world declared: no stray or missing rows.
#[tokio::test]
async fn stored_grants_are_the_declared_relations() {
    let fixture = Fixture::new().await;
    let store = fixture.database.repositories();
    for repo in &fixture.world.repos {
        let owned = store.find_by_path(repo.shape.owner(), repo.shape.name()).await.unwrap().unwrap();
        for (actor, user) in &fixture.world.users {
            let declared = relation(*actor, repo.shape).unwrap();
            if declared.site_admin || declared.repository_owner {
                continue; // Their grants are never looked up.
            }
            let stored = store.grants(repo.id, owned.repo.owner_id, user.id()).await.unwrap();
            let actual = Grants {
                org_role: stored.org_role,
                org_base: stored.org_base.permission(),
                team: stored.team,
                collaborator: stored.collaborator,
            };
            let expected = grants(declared);
            // A base permission only matters with membership; nonmembers may see the org's value.
            assert_eq!(
                (actual.org_role, actual.team, actual.collaborator),
                (expected.org_role, expected.team, expected.collaborator),
                "{}",
                label(*actor, repo.shape)
            );
            if expected.org_role.is_some() {
                assert_eq!(actual.org_base, expected.org_base, "{}", label(*actor, repo.shape));
            }
        }
    }
}

#[tokio::test]
async fn every_open_path_decides_like_the_oracle() {
    let fixture = Fixture::new().await;
    let repositories = &fixture.services.repositories;
    for shape in SHAPES {
        for actor in ACTORS {
            let viewer = fixture.world.user(actor);
            let (owner, name) = (shape.owner(), shape.name());
            let expected = oracle(actor, shape);
            let (opened, ctx) = Fixture::opened(repositories.open(viewer, owner, name).await);
            assert_eq!(opened, expected, "open: {}", label(actor, shape));
            let (renamed, _) = Fixture::opened(repositories.open_following_renames(viewer, owner, name).await);
            assert_eq!(renamed, expected, "open_following_renames: {}", label(actor, shape));
            let page = repositories.open_page(viewer, owner, name).await.map(|(ctx, _)| ctx);
            assert_eq!(Fixture::opened(page).0, expected, "open_page: {}", label(actor, shape));
            if let Some(ctx) = ctx {
                let writable = expected.can_write() && !shape.archived;
                assert_eq!(ctx.can_write_content(), writable, "can_write_content: {}", label(actor, shape));
                assert_eq!(ctx.ensure_writable().is_ok(), !shape.archived, "{}", label(actor, shape));
                assert_eq!(fixture.services.edits.can_edit(&ctx).await.unwrap(), writable, "can_edit: {}", label(actor, shape));
            }
        }
    }
}

#[tokio::test]
async fn listings_and_search_omit_what_the_viewer_cannot_read() {
    let fixture = Fixture::new().await;
    let accounts = fixture.database.accounts();
    let owners =
        [accounts.find_account_by_name("alice").await.unwrap().unwrap(), accounts.find_account_by_name("acme").await.unwrap().unwrap()];
    for actor in ACTORS {
        let viewer = fixture.world.user(actor);
        let readable: BTreeSet<String> =
            SHAPES.into_iter().filter(|shape| oracle(actor, *shape).can_read()).map(Shape::full_name).collect();
        let mut listed = BTreeSet::new();
        for owner in &owners {
            let repos = fixture.services.repositories.list_for_owner(viewer, owner).await.unwrap();
            listed.extend(repos.iter().map(rbit_core::OwnedRepository::full_name));
        }
        assert_eq!(listed, readable, "list_for_owner for {actor:?}");
    }
    // Explore and search take no viewer: only public repositories, archived ones included.
    let search = RepoSearch { text: String::new(), sort: RepoSort::default(), archived: ArchivedFilter::Include, page: 1 };
    let found: BTreeSet<String> =
        fixture.services.repositories.explore(&search).await.unwrap().repos.iter().map(rbit_core::OwnedRepository::full_name).collect();
    let public: BTreeSet<String> = SHAPES.into_iter().filter(|shape| shape.public).map(Shape::full_name).collect();
    assert_eq!(found, public);
    let search = RepoSearch { text: "closed".to_owned(), ..search };
    assert!(fixture.services.repositories.explore(&search).await.unwrap().repos.is_empty(), "private names never match");
}

/// Issue and label writes: reading suffices for an issue, writing for a label, and
/// archived repositories refuse both after the access check.
#[tokio::test]
async fn content_writes_follow_the_oracle_and_the_archive_guard() {
    let fixture = Fixture::new().await;
    for shape in SHAPES {
        for (actor, user) in &fixture.world.users {
            let expected = oracle(*actor, shape);
            let Ok(ctx) = fixture.services.repositories.open(Some(user), shape.owner(), shape.name()).await else {
                assert!(!expected.can_read(), "{}", label(*actor, shape));
                continue;
            };
            let issue = NewIssueInput { title: format!("{actor:?} report"), ..NewIssueInput::default() };
            let created = fixture.services.issues.create(user, &ctx, issue).await;
            match (shape.archived, created) {
                (false, Ok(_)) | (true, Err(ServiceError::Archived)) => {}
                (_, other) => panic!("issue by {}: {other:?}", label(*actor, shape)),
            }
            let input = LabelInput { name: format!("{actor:?}"), color: "ededed".to_owned(), description: String::new() };
            let created = fixture.services.issues.create_label(user, &ctx, &input).await;
            match (expected.can_write(), shape.archived, created) {
                (true, false, Ok(_)) | (true, true, Err(ServiceError::Archived)) | (false, _, Err(ServiceError::Forbidden)) => {}
                (_, _, other) => panic!("label by {}: {other:?}", label(*actor, shape)),
            }
        }
    }
}

/// Suspension is enforced where credentials are resumed, before any policy decision: a
/// session and a token issued before it stop working, and the next request is refused.
#[tokio::test]
async fn suspension_refuses_credentials_issued_before_it() {
    let fixture = Fixture::new().await;
    let sam: &User = fixture.world.user(Actor::Suspended).unwrap();
    let session = fixture.services.accounts.start_session(sam).await.unwrap();
    let input = CreateAccessToken { name: "probe".to_owned(), scope: rbit_core::TokenScope::Admin, expires_in_days: None };
    let (_, token) = fixture.services.tokens.create(sam, input).await.unwrap();
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_some());
    assert!(fixture.services.tokens.authenticate(&token).await.unwrap().is_some());
    fixture.world.suspend(&fixture.database).await;
    assert!(fixture.services.accounts.is_suspended(sam.id()).await.unwrap());
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_none());
    assert!(fixture.services.tokens.authenticate(&token).await.unwrap().is_none());
}
