//! A test fixture for the milestone 3 API areas (`collaborators`, `teams`, `releases`,
//! `forks`): an isolated instance, its services and the full router, driven in-process.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    http::{Request, Response, header},
};
use rbit_core::{Config, TokenScope, User, Visibility};
use rbit_services::{CreateAccessToken, CreateRepository, Services, SignUp};
use rbit_storage::{Database, KvStore};
use serde_json::Value;
use tower::ServiceExt;

use crate::AppState;

pub struct Fixture {
    _dir: tempfile::TempDir,
    pub services: Services,
    router: Router,
}

impl Fixture {
    pub async fn new(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        "https://git.example.com".clone_into(&mut config.server.public_url);
        config.security.rate_limit = false;
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let router = crate::router(AppState::new(config, services.clone()));
        Self { _dir: dir, services, router }
    }

    pub async fn user(&self, name: &str) -> User {
        let signup = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
        self.services.accounts.create_user(signup, Some(false)).await.unwrap()
    }

    pub async fn token(&self, user: &User, scope: TokenScope) -> String {
        let input = CreateAccessToken { name: format!("{scope:?}"), scope, expires_in_days: None };
        self.services.tokens.create(user, input).await.unwrap().1
    }

    pub async fn repo(&self, owner: &User, name: &str, visibility: Visibility) {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility };
        self.services.repositories.create(owner, input).await.unwrap();
    }

    /// A JSON request; the response body parsed as JSON (`Null` when empty).
    pub async fn send(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (Response<Body>, Value) {
        let content_type = body.as_ref().map(|_| "application/json");
        let (response, bytes) = self.raw(method, path, token, content_type, body.map(|body| body.to_string().into_bytes())).await;
        let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
        (response, json)
    }

    /// Any request; the response body as bytes.
    pub async fn raw(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        content_type: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> (Response<Body>, Bytes) {
        self.request(method, path, token, &content_type.map(|value| vec![(header::CONTENT_TYPE.as_str(), value)]).unwrap_or_default(), body)
            .await
    }

    /// A prepared request, answered by the full router.
    pub async fn oneshot(&self, request: Request<Body>) -> Response<Body> {
        self.router.clone().oneshot(request).await.unwrap()
    }

    /// Any request with extra headers; the response body as bytes.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> (Response<Body>, Bytes) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let request = request.body(body.map_or_else(Body::empty, Body::from)).unwrap();
        let (parts, body) = self.oneshot(request).await.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        (Response::from_parts(parts, Body::empty()), bytes)
    }
}
