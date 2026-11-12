//! Mandatory permission regression group, HTTP ingresses: browser pages and forms,
//! browser edits, the REST API and Git smart HTTP decide every actor and repository of the
//! shared world (`rbit-services/tests/support/permission_matrix.rs`) exactly as its oracle
//! says, through the real router, sessions, tokens and Git. Credentials are issued first,
//! then the suspended account is suspended, so every ingress sees a refused credential.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    body::{Body, Bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_core::{Config, TokenScope};
use rbit_services::{CreateAccessToken, Services};
use rbit_storage::{Database, KvStore};
use tower::ServiceExt;

use crate::permission_world::{Actor, World};

mod api;
mod browser;
mod caching;
mod comments;
mod git_http;
mod tasks;

/// The branch prefix of the refs Git smart HTTP may delete.
const PROBE: &str = "http";

pub struct Fixture {
    _dir: tempfile::TempDir,
    pub database: Database,
    router: Router,
    pub world: World,
    cookies: HashMap<Actor, String>,
    tokens: HashMap<Actor, String>,
}

/// A response's status, headers and body.
pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Answer {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Where a redirect leads.
    pub fn location(&self) -> Option<&str> {
        self.headers.get(header::LOCATION).and_then(|value| value.to_str().ok())
    }
}

/// How a request authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    /// The session cookie, as a browser sends it.
    Session,
    /// `Authorization: Bearer` with a personal access token (`admin` scope), as API clients do.
    Bearer,
    /// `Authorization: Basic` with the token as password, as Git does.
    Basic,
}

impl Fixture {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        config.security.rate_limit = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let world = World::build(&services, &database, dir.path(), &[PROBE]).await;
        let (mut cookies, mut tokens) = (HashMap::new(), HashMap::new());
        for (actor, user) in &world.users {
            let session = services.accounts.start_session(user).await.unwrap();
            cookies.insert(*actor, format!("rbit_session={}", session.encode()));
            let input = CreateAccessToken { name: "matrix".to_owned(), scope: TokenScope::Admin, expires_in_days: None };
            tokens.insert(*actor, services.tokens.create(user, input).await.unwrap().1);
        }
        world.suspend(&database).await;
        let router = rbit_web::router(rbit_web::AppState::new(config, services.clone()));
        Self { _dir: dir, database, router, world, cookies, tokens }
    }

    /// The request builder for `actor` with `credential`; anonymous requests carry none.
    pub fn request(&self, method: Method, path: &str, actor: Actor, credential: Credential) -> axum::http::request::Builder {
        let mut request = Request::builder().method(method).uri(path).header("sec-fetch-site", "same-origin");
        match credential {
            Credential::Session => {
                if let Some(cookie) = self.cookies.get(&actor) {
                    request = request.header(header::COOKIE, cookie);
                }
            }
            Credential::Bearer => {
                if let Some(token) = self.tokens.get(&actor) {
                    request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
                }
            }
            Credential::Basic => {
                if let Some(token) = self.tokens.get(&actor) {
                    request = request.header(header::AUTHORIZATION, format!("Basic {}", STANDARD.encode(format!("x:{token}"))));
                }
            }
        }
        request
    }

    pub async fn send(&self, request: axum::http::request::Builder, body: impl Into<Body>) -> Answer {
        let response = self.router.clone().oneshot(request.body(body.into()).unwrap()).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Answer { status, headers, body }
    }

    /// A body-less request.
    pub async fn get(&self, path: &str, actor: Actor, credential: Credential) -> Answer {
        self.send(self.request(Method::GET, path, actor, credential), Body::empty()).await
    }

    /// A browser form post.
    pub async fn post_form(&self, path: &str, actor: Actor, form: &str) -> Answer {
        let request =
            self.request(Method::POST, path, actor, Credential::Session).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        self.send(request, form.to_owned()).await
    }

    /// An API request with an optional JSON body.
    pub async fn api(&self, method: Method, path: &str, actor: Actor, body: Option<serde_json::Value>) -> Answer {
        let mut request = self.request(method, &format!("/api/v1{path}"), actor, Credential::Bearer);
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        self.send(request, body.map_or_else(String::new, |body| body.to_string())).await
    }
}

/// Whether `answer` sends a signed-out browser to the sign-in page.
pub fn asks_to_sign_in(answer: &Answer) -> bool {
    answer.status.is_redirection() && answer.location().is_some_and(|location| location.starts_with("/login"))
}
