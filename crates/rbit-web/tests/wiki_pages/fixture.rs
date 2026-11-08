//! A router over a temporary instance: one repository with its wiki on, four people.
use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use rbit_core::{Config, Permission, Timestamp, User, Visibility};
use rbit_localization::{Locale, LocaleResolver};
use rbit_services::{CreateRepository, RepoContext, Services, wikis::publish::WikiPageInput};
use rbit_storage::{Database, KvStore, NewUser};
use tower::ServiceExt;

pub struct Response {
    pub status: StatusCode,
    pub location: Option<String>,
    pub body: String,
}

pub struct Fixture {
    _directory: tempfile::TempDir,
    pub database: Database,
    pub services: Services,
    router: Router,
    pub owner: User,
    pub writer: User,
    pub reader: User,
    pub outsider: User,
}

impl Fixture {
    pub async fn new(visibility: Visibility) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let mut people = Vec::new();
        for name in ["owner", "writer", "reader", "outsider"] {
            let user = database
                .accounts()
                .create_user(NewUser {
                    name: name.parse().unwrap(),
                    email: format!("{name}@example.com").parse().unwrap(),
                    display_name: None,
                    password_hash: "synthetic wiki fixture".into(),
                    is_admin: false,
                })
                .await
                .unwrap();
            people.push(user);
        }
        let [owner, writer, reader, outsider] = <[User; 4]>::try_from(people).unwrap();
        services
            .repositories
            .create(&owner, CreateRepository { name: "handbook".into(), description: String::new(), visibility })
            .await
            .unwrap();
        let repo = services.repositories.open(Some(&owner), "owner", "handbook").await.unwrap();
        services.collaborators.add(&owner, &repo, "writer", Permission::Write).await.unwrap();
        services.collaborators.add(&owner, &repo, "reader", Permission::Read).await.unwrap();
        services.wikis.set_enabled(&repo, true).await.unwrap();
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()))
            .layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
        Self { _directory: directory, database, services, router, owner, writer, reader, outsider }
    }

    pub async fn repo(&self, user: &User) -> RepoContext {
        self.services.repositories.open(Some(user), "owner", "handbook").await.unwrap()
    }

    /// Creates a page through the service, as `writer`.
    pub async fn page(&self, title: &str, content: &str) {
        let input = WikiPageInput { title: title.into(), path: String::new(), content: content.into() };
        self.services.wikis.create(&self.repo(&self.writer).await, &self.writer, &input).await.unwrap();
    }

    pub async fn archive(&self) {
        let id = self.repo(&self.owner).await.repo.repo.id;
        self.database.repo_settings().archive(id, self.owner.id(), Timestamp(2)).await.unwrap();
    }

    pub async fn cookie(&self, user: &User) -> String {
        format!("{}={}", rbit_web::middleware::SESSION_COOKIE, self.services.accounts.start_session(user).await.unwrap().encode())
    }

    async fn send(&self, request: Request<Body>) -> Response {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Response { status, location, body: String::from_utf8_lossy(&body).into_owned() }
    }

    pub async fn get(&self, path: &str, user: Option<&User>) -> Response {
        self.get_with(path, user, &[]).await
    }

    pub async fn get_with(&self, path: &str, user: Option<&User>, headers: &[(&str, &str)]) -> Response {
        let mut request = Request::builder().uri(path).header(header::HOST, "localhost");
        if let Some(user) = user {
            request = request.header(header::COOKIE, self.cookie(user).await);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.send(request.body(Body::empty()).unwrap()).await
    }

    pub async fn post(&self, path: &str, user: Option<&User>, pairs: &[(&str, &str)]) -> Response {
        let encode = |value: &str| percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string();
        let body = pairs.iter().map(|(key, value)| format!("{}={}", encode(key), encode(value))).collect::<Vec<_>>().join("&");
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, "http://localhost")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(user) = user {
            request = request.header(header::COOKIE, self.cookie(user).await);
        }
        self.send(request.body(Body::from(body)).unwrap()).await
    }
}
