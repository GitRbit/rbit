//! Actions workstream U: the Actions settings of a repository, an organisation and a user's
//! own account — who may see and change them, the cards and navigation entry that lead
//! there, secrets that are never rendered back, escaped variables and trusted authors.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{
    Config, Permission, RepoId, Timestamp, User,
    actions::{DefaultTokenPermission, SealedSecret, SecretScope, SecretVisibility},
};
use rbit_services::{CreateOrganization, CreateRepository, Services, SignUp, actions::secrets::ActionsTarget};
use rbit_storage::{Database, KvStore};
use rbit_web::AppState;
use tower::ServiceExt;

const REPO: &str = "/alice/rocket/settings/actions";

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    services: Services,
    router: Router,
    alice: User,
    bob: User,
    /// Alice's session cookie: she owns `alice/rocket` (public) and the organisation `acme`.
    owner: String,
    /// Carol's: she may write to `alice/rocket`.
    writer: String,
    /// Bob's: he may read the repository, nothing more.
    reader: String,
}

impl Fixture {
    async fn new() -> Self {
        Self::with(true).await
    }

    async fn with(actions: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.actions.enabled = actions;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let sign_up =
            |name: &str| SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        let alice = services.accounts.create_user(sign_up("alice"), Some(true)).await.unwrap();
        let bob = services.accounts.create_user(sign_up("bob"), Some(false)).await.unwrap();
        let carol = services.accounts.create_user(sign_up("carol"), Some(false)).await.unwrap();
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        let repo = services.repositories.open(Some(&alice), "alice", "rocket").await.unwrap();
        services.collaborators.add(&alice, &repo, "carol", Permission::Write).await.unwrap();
        let org = CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() };
        services.orgs.create(&alice, org).await.unwrap();
        let session = |user: User| {
            let services = services.clone();
            async move { format!("rbit_session={}", services.accounts.start_session(&user).await.unwrap().encode()) }
        };
        let (owner, writer, reader) = (session(alice.clone()).await, session(carol).await, session(bob.clone()).await);
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, database, services, router, alice, bob, owner, writer, reader }
    }

    async fn send(&self, method: Method, path: &str, cookie: &str, body: &str) -> (StatusCode, String, Option<String>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, cookie)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned(), location)
    }

    async fn page(&self, path: &str, cookie: &str) -> String {
        let (status, body, _) = self.send(Method::GET, path, cookie, "").await;
        assert_eq!(status, StatusCode::OK, "{path}");
        body
    }

    /// Posts `body` and expects a redirect to `to`.
    async fn redirects(&self, path: &str, cookie: &str, body: &str, to: &str) {
        let (status, page, location) = self.send(Method::POST, path, cookie, body).await;
        assert!(status.is_redirection(), "{path} {body}: {status} {page}");
        assert_eq!(location.as_deref(), Some(to), "{path} {body}");
    }

    async fn repo_id(&self) -> RepoId {
        self.database.repositories().find_by_path("alice", "rocket").await.unwrap().unwrap().repo.id
    }

    fn actions(&self) -> rbit_services::ActionsService {
        self.services.actions.snapshot().unwrap()
    }
}

#[tokio::test]
async fn the_cards_and_the_navigation_entry_lead_to_the_pages() {
    let fixture = Fixture::new().await;
    let settings = fixture.page("/alice/rocket/settings", &fixture.owner).await;
    assert!(settings.contains(&format!(r#"href="{REPO}""#)), "the repository settings card");
    let org = fixture.page("/acme/settings", &fixture.owner).await;
    assert!(org.contains(r#"href="/acme/settings/actions""#), "the organisation settings card");
    for cookie in [&fixture.owner, &fixture.reader] {
        let profile = fixture.page("/settings/profile", cookie).await;
        assert!(profile.contains(r#"href="/settings/actions""#), "the personal settings entry");
    }
    let personal = fixture.page("/settings/actions", &fixture.reader).await;
    assert!(personal.contains(r#"href="/settings/actions" aria-current="page""#));
}

#[tokio::test]
async fn without_actions_on_the_instance_every_settings_page_is_missing() {
    let fixture = Fixture::with(false).await;
    for path in [
        REPO,
        "/alice/rocket/settings/actions/secrets",
        "/alice/rocket/settings/actions/variables",
        "/alice/rocket/settings/actions/trusted",
        "/acme/settings/actions",
        "/settings/actions",
    ] {
        for method in [Method::GET, Method::POST] {
            let (status, ..) = fixture.send(method.clone(), path, &fixture.owner, "kind=secret&name=A&value=b").await;
            assert!(matches!(status, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED), "{method} {path}: {status}");
        }
    }
    let settings = fixture.page("/alice/rocket/settings", &fixture.owner).await;
    assert!(!settings.contains("/settings/actions"));
    let profile = fixture.page("/settings/profile", &fixture.owner).await;
    assert!(!profile.contains("/settings/actions"));
}

#[tokio::test]
async fn only_administrators_get_the_repository_page() {
    let fixture = Fixture::new().await;
    let page = fixture.page(REPO, &fixture.owner).await;
    assert!(page.contains(r#"hx-history="false""#), "secret pages stay out of the history cache");
    assert!(page.contains(r#"action="/alice/rocket/settings/actions/secrets""#));
    assert!(page.contains(r#"action="/alice/rocket/settings/actions/variables""#));

    let posts = [
        (REPO, "enabled=on&token_permission=write"),
        ("/alice/rocket/settings/actions/secrets", "name=TOKEN&value=x"),
        ("/alice/rocket/settings/actions/variables", "name=REGION&value=x"),
        ("/alice/rocket/settings/actions/trusted", "user=1"),
    ];
    for cookie in [&fixture.writer, &fixture.reader, ""] {
        let (status, ..) = fixture.send(Method::GET, REPO, cookie, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET as {cookie:?}");
        for (path, body) in posts {
            let (status, _, location) = fixture.send(Method::POST, path, cookie, body).await;
            if cookie.is_empty() {
                assert!(location.is_some_and(|location| location.starts_with("/login")), "POST {path} signed out");
            } else {
                assert_eq!(status, StatusCode::NOT_FOUND, "POST {path} as {cookie:?}");
            }
        }
    }
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    assert!(fixture.actions().list_secrets(ActionsTarget::Repository(&repo)).await.unwrap().is_empty(), "nothing was stored");
    assert!(fixture.actions().list_variables(ActionsTarget::Repository(&repo)).await.unwrap().is_empty(), "nor variables");
    assert_eq!(fixture.actions().repo_settings(&repo).await.unwrap().token_permission, DefaultTokenPermission::Read);

    for path in ["/alice/rocket/settings/actions/secrets", "/alice/rocket/settings/actions/variables"] {
        let (status, _, location) = fixture.send(Method::GET, path, &fixture.owner, "").await;
        assert!(status.is_redirection(), "{path}");
        assert_eq!(location.as_deref(), Some(REPO));
    }
    let (status, ..) = fixture.send(Method::GET, "/alice/missing/settings/actions", &fixture.owner, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_switch_and_the_token_permission_are_saved() {
    let fixture = Fixture::new().await;
    let page = fixture.page(REPO, &fixture.owner).await;
    assert!(page.contains(r#"name="enabled" value="on" class="mt-1 accent-(--ink)" >"#), "off for a new repository");
    assert!(page.contains(r#"value="read" class="mt-1 accent-(--ink)" checked"#));

    fixture.redirects(REPO, &fixture.owner, "enabled=on&token_permission=write", &format!("{REPO}?done=saved")).await;
    let page = fixture.page(&format!("{REPO}?done=saved"), &fixture.owner).await;
    assert!(page.contains("Settings saved."));
    assert!(page.contains(r#"name="enabled" value="on" class="mt-1 accent-(--ink)" checked"#));
    assert!(page.contains(r#"value="write" class="mt-1 accent-(--ink)" checked"#));
    let home = fixture.page("/alice/rocket", &fixture.reader).await;
    assert!(home.contains(r#"data-repo-tab="rbit.actions:actions-repository-tab""#), "the tab follows the switch");

    fixture.redirects(REPO, &fixture.owner, "token_permission=write", &format!("{REPO}?done=saved")).await;
    assert!(!fixture.database.actions_settings().is_enabled(fixture.repo_id().await).await.unwrap(), "an unticked box turns it off");

    let (status, page, _) = fixture.send(Method::POST, REPO, &fixture.owner, "enabled=on&token_permission=admin").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("Choose read or read and write."));
    let page = fixture.page(&format!("{REPO}?done=%3Cscript%3E"), &fixture.owner).await;
    assert!(!page.contains("script&#62;") && !page.contains(r#"role="status""#), "unknown notices show nothing");
}

#[tokio::test]
async fn secret_values_are_never_rendered_back() {
    let fixture = Fixture::new().await;
    let secrets = "/alice/rocket/settings/actions/secrets";
    let done = format!("{REPO}?done=secret-saved");
    fixture.redirects(secrets, &fixture.owner, "name=deploy_key&value=hunter2-value%0D%0Aline", &done).await;
    let page = fixture.page(&done, &fixture.owner).await;
    assert!(page.contains("Secret saved.") && page.contains(">DEPLOY_KEY<"));
    assert!(!page.contains("hunter2"), "the value is never shown");
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    assert_eq!(fixture.actions().list_secrets(ActionsTarget::Repository(&repo)).await.unwrap().len(), 1);

    // A rejected form keeps the name, never the value.
    let (status, page, _) = fixture.send(Method::POST, secrets, &fixture.owner, "name=1bad%3Cname&value=hunter3-value").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("digit"), "the name's problem is shown");
    assert!(page.contains(r#"value="1bad&#60;name""#), "the name is kept, escaped");
    assert!(!page.contains("hunter3"));
    let (status, page, _) = fixture.send(Method::POST, secrets, &fixture.owner, "name=EMPTY&value=").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("A secret needs a value."));

    fixture.redirects(secrets, &fixture.owner, "action=delete&name=DEPLOY_KEY", &format!("{REPO}?done=secret-deleted")).await;
    assert!(fixture.actions().list_secrets(ActionsTarget::Repository(&repo)).await.unwrap().is_empty(), "deleted");
    let (status, ..) = fixture.send(Method::POST, secrets, &fixture.owner, "action=delete&name=DEPLOY_KEY").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unreadable_secrets_and_a_missing_key_are_explained() {
    let fixture = Fixture::new().await;
    // Stored before the key was ever looked at: the key file is then never created.
    let sealed = |_| SealedSecret { key_id: [9; 8], nonce: [0; 12], ciphertext: vec![0; 21] };
    let scope = SecretScope::Repository(fixture.repo_id().await);
    let store = fixture.database.actions_secrets();
    store.put_secret(scope, &"OLD".parse().unwrap(), SecretVisibility::All, fixture.alice.id(), Timestamp::now(), sealed).await.unwrap();

    let page = fixture.page(REPO, &fixture.owner).await;
    assert!(page.contains("Cannot be decrypted — set it again"));
    assert!(page.contains("Secrets cannot be stored or used:") && page.contains("missing while secrets are stored"));
    let (status, page, _) = fixture.send(Method::POST, "/alice/rocket/settings/actions/secrets", &fixture.owner, "name=NEW&value=v").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("Secrets cannot be stored"));
    // Deleting works without the key.
    let done = format!("{REPO}?done=secret-deleted");
    fixture.redirects("/alice/rocket/settings/actions/secrets", &fixture.owner, "action=delete&name=OLD", &done).await;
}

#[tokio::test]
async fn variables_are_shown_escaped_and_can_be_edited() {
    let fixture = Fixture::new().await;
    let variables = "/alice/rocket/settings/actions/variables";
    let done = format!("{REPO}?done=variable-saved");
    fixture.redirects(variables, &fixture.owner, "name=greeting&value=%3Cb%3Ehi%3C%2Fb%3E", &done).await;
    let page = fixture.page(&done, &fixture.owner).await;
    assert!(page.contains(">GREETING<"));
    assert!(page.contains("&#60;b&#62;hi&#60;/b&#62;") && !page.contains("<b>hi</b>"), "values are escaped");

    fixture.redirects(variables, &fixture.owner, "action=put&name=GREETING&value=hello", &done).await;
    let page = fixture.page(REPO, &fixture.owner).await;
    assert!(page.contains(">hello<") && !page.contains("&#60;b&#62;"));
    let (status, page, _) = fixture.send(Method::POST, variables, &fixture.owner, "name=GITHUB_X&value=kept-value").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("GITHUB_") && page.contains("kept-value"), "a rejected variable is kept");

    fixture.redirects(variables, &fixture.owner, "action=delete&name=greeting", &format!("{REPO}?done=variable-deleted")).await;
    assert!(fixture.page(REPO, &fixture.owner).await.contains("No variables"));
}

#[tokio::test]
async fn trusted_authors_can_be_removed() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo_id().await;
    fixture.database.actions_settings().trust(repo, fixture.bob.id(), fixture.alice.id(), Timestamp::now()).await.unwrap();
    let page = fixture.page(REPO, &fixture.owner).await;
    assert!(page.contains(r#"href="/bob""#) && page.contains("by alice"));
    assert!(page.contains(&format!(r#"name="user" value="{}""#, fixture.bob.id().0)));

    let trusted = "/alice/rocket/settings/actions/trusted";
    let body = format!("user={}", fixture.bob.id().0);
    fixture.redirects(trusted, &fixture.owner, &body, &format!("{REPO}?done=untrusted")).await;
    assert!(!fixture.database.actions_settings().is_trusted(repo, fixture.bob.id()).await.unwrap());
    assert!(fixture.page(REPO, &fixture.owner).await.contains("No trusted authors"));
    for body in [body.as_str(), "user=nope", ""] {
        let (status, ..) = fixture.send(Method::POST, trusted, &fixture.owner, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }
}

#[tokio::test]
async fn organisation_owners_give_secrets_to_selected_repositories() {
    let fixture = Fixture::new().await;
    let url = "/acme/settings/actions";
    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains(r#"hx-history="false""#) && page.contains("This owner has no repositories yet."));
    for cookie in [&fixture.reader, &fixture.writer] {
        let (status, ..) = fixture.send(Method::GET, url, cookie, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, ..) = fixture.send(Method::POST, url, cookie, "kind=variable&name=A&value=b").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let (status, ..) = fixture.send(Method::GET, "/alice/settings/actions", &fixture.owner, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a user's are under personal settings");

    let org = fixture.services.orgs.open(Some(&fixture.alice), "acme").await.unwrap();
    let input = CreateRepository { name: "anvil".into(), description: String::new(), visibility: rbit_core::Visibility::Private };
    fixture.services.repositories.create_in(&fixture.alice, "acme", input).await.unwrap();
    let anvil = fixture.database.repositories().find_by_path("acme", "anvil").await.unwrap().unwrap().repo.id;
    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains(&format!(r#"name="repository" value="{}""#, anvil.0)) && page.contains(">anvil<"));

    let body = format!("kind=secret&action=put&name=deploy&value=org-secret-value&visibility=selected&repository={}", anvil.0);
    fixture.redirects(url, &fixture.owner, &body, &format!("{url}?done=secret-saved")).await;
    let listed = fixture.actions().list_secrets(ActionsTarget::Organization(&org)).await.unwrap();
    assert_eq!(listed[0].info.visibility, SecretVisibility::Selected);
    assert_eq!(listed[0].repositories, [anvil]);
    let page = fixture.page(url, &fixture.owner).await;
    assert!(page.contains("1 selected repository") && !page.contains("org-secret-value"));

    fixture.redirects(url, &fixture.owner, "kind=secret&action=repositories&name=DEPLOY", &format!("{url}?done=secret-saved")).await;
    let listed = fixture.actions().list_secrets(ActionsTarget::Organization(&org)).await.unwrap();
    assert!(listed[0].repositories.is_empty(), "no repository ticked, the value kept");
    fixture.redirects(url, &fixture.owner, "kind=variable&name=region&value=eu", &format!("{url}?done=variable-saved")).await;
    assert!(fixture.page(url, &fixture.owner).await.contains(">REGION<"));
    let (status, page, _) = fixture.send(Method::POST, url, &fixture.owner, "name=X&value=y").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "a form without its kind");
    assert!(page.contains("The form is incomplete"));
}

#[tokio::test]
async fn people_manage_their_own_secrets_and_variables() {
    let fixture = Fixture::new().await;
    let url = "/settings/actions";
    let (status, _, location) = fixture.send(Method::GET, url, "", "").await;
    assert!(status.is_redirection() && location.is_some_and(|location| location.starts_with("/login")));
    let page = fixture.page(url, &fixture.reader).await;
    assert!(page.contains(r#"hx-history="false""#) && page.contains("No secrets"));

    fixture.redirects(url, &fixture.reader, "kind=secret&name=npm_token&value=bobs-value", &format!("{url}?done=secret-saved")).await;
    fixture.redirects(url, &fixture.reader, "kind=variable&name=node&value=24", &format!("{url}?done=variable-saved")).await;
    let page = fixture.page(url, &fixture.reader).await;
    assert!(page.contains(">NPM_TOKEN<") && page.contains("All repositories") && !page.contains("bobs-value"));
    assert!(page.contains(">NODE<") && page.contains(">24<"));
    let alices = fixture.page(url, &fixture.owner).await;
    assert!(!alices.contains("NPM_TOKEN") && !alices.contains(">NODE<"), "each account has its own");

    let listed = fixture.actions().list_secrets(ActionsTarget::User(&fixture.bob)).await.unwrap();
    assert_eq!(listed.len(), 1);
    fixture.redirects(url, &fixture.reader, "kind=secret&action=delete&name=NPM_TOKEN", &format!("{url}?done=secret-deleted")).await;
    assert!(fixture.actions().list_secrets(ActionsTarget::User(&fixture.bob)).await.unwrap().is_empty(), "deleted");
}
