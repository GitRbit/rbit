//! The command-line client: `rbit auth`, `rbit run`, `rbit workflow`, `rbit artifact`,
//! `rbit pr` and `rbit api`, which talk to an rbit instance's REST API as `gh` talks to
//! GitHub's.
//!
//! * The instance and its token come, in this order, from `--hostname` / `RBIT_HOST` and
//!   `RBIT_TOKEN` (or `GH_TOKEN`), from the logins `rbit auth login` stored, and from the
//!   `origin` remote of the Git repository in the current directory.
//! * The repository is `--repo owner/name`, or `origin`'s. A remote on another host than
//!   the logged-in one is used with that host's login.
//! * Logins live in `hosts.toml` in the user's configuration directory
//!   (`$XDG_CONFIG_HOME/rbit`, `~/.config/rbit`, or `%APPDATA%\rbit`), readable only by the
//!   user on Unix. A token is never printed and never put in a URL.
//!
//! The client is part of the `rbit` binary, which every release already builds for every
//! platform, so it always speaks the API of the server it ships with.

mod commands;
mod hosts;

use std::time::Duration;

use anyhow::{Context as _, anyhow, bail};
use reqwest::{Method, StatusCode, Url};
use serde_json::Value;

pub use commands::{ClientCommand, run};

/// An authenticated connection to one instance's REST API.
pub struct Api {
    http: reqwest::Client,
    /// Validated instance origin and optional deployment path; no credentials.
    base: Url,
    token: Option<String>,
}

/// A repository, `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl Repo {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let (owner, name) = text.trim().split_once('/').ok_or_else(|| anyhow!("`{text}` is not `owner/name`"))?;
        let name = name.trim_end_matches(".git");
        let valid = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        if !valid(owner) || !valid(name) {
            bail!("`{text}` is not `owner/name`");
        }
        Ok(Self { owner: owner.to_owned(), name: name.to_owned() })
    }
}

/// An instance and repository from a Git remote URL: `https://host[:port]/owner/name(.git)`,
/// `ssh://git@host[:port]/owner/name.git` or `git@host:owner/name.git`.
pub fn parse_remote(url: &str) -> Option<(String, Repo)> {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        let (user_host, path) = url.split_once(':')?;
        let host = user_host.rsplit('@').next()?;
        return Some((format!("https://{host}"), Repo::parse(path.trim_start_matches('/')).ok()?));
    };
    let rest = rest.rsplit_once('@').map_or(rest, |(_, after)| after);
    let (host, path) = rest.split_once('/')?;
    let base = match scheme {
        "http" | "https" => format!("{scheme}://{host}"),
        // SSH's port is not the web's: the web is on the default HTTPS port.
        "ssh" | "git" => format!("https://{}", host.split(':').next()?),
        _ => return None,
    };
    let path = path.trim_end_matches('/');
    // The repository is the last two segments, so instances served below a path work too.
    let mut parts = path.rsplitn(3, '/');
    let name = parts.next()?;
    let owner = parts.next()?;
    let prefix = parts.next().map(|prefix| format!("/{prefix}")).unwrap_or_default();
    Some((format!("{base}{prefix}"), Repo::parse(&format!("{owner}/{name}")).ok()?))
}

impl Api {
    pub fn new(base: &str, token: Option<String>) -> anyhow::Result<Self> {
        validate_url_text(base)?;
        let base = Url::parse(base).context("invalid instance URL")?;
        validate_http_url(&base)?;
        if base.query().is_some() {
            bail!("instance URLs cannot contain a query");
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("rbit-cli/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_mins(5))
            // Never silently replay authentication or request bodies elsewhere,
            // including an unauthenticated login or a future streamed upload.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("cannot set up HTTPS")?;
        Ok(Self { http, base, token })
    }

    /// `path` (below `/api/v1`, or a full URL of this instance) as a request.
    fn request(&self, method: Method, path: &str) -> anyhow::Result<reqwest::RequestBuilder> {
        let url = if let Ok(url) = Url::parse(path) {
            validate_url_text(path)?;
            url
        } else {
            // Reject authority-relative references and malformed absolute
            // URLs rather than interpreting them as API path fragments.
            let first = path.split(['/', '?', '#']).next().unwrap_or_default();
            if path.starts_with("//") || first.contains(':') {
                bail!("invalid API destination");
            }
            validate_path(path.split('?').next().unwrap_or_default())?;
            if path.contains('#') || path.chars().any(char::is_control) {
                bail!("invalid API destination");
            }
            Url::parse(&format!("{}/api/v1/{}", self.base.as_str().trim_end_matches('/'), path.trim_start_matches('/')))
                .context("invalid API destination")?
        };
        validate_http_url(&url)?;
        let prefix = percent_case(self.base.path().trim_end_matches('/'));
        let path = percent_case(url.path());
        let within_path = path == prefix || path.strip_prefix(prefix.as_slice()).is_some_and(|rest| rest.starts_with(b"/"));
        if url.origin() != self.base.origin() || !within_path {
            bail!("API destination is outside the configured instance");
        }
        let request = self.http.request(method, url).header("Accept", "application/json");
        Ok(match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        })
    }

    /// The JSON answer of a request, or an error with the server's message.
    pub async fn json(&self, method: Method, path: &str, body: Option<&Value>) -> anyhow::Result<Value> {
        let mut request = self.request(method, path)?;
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.with_context(|| format!("cannot reach {}", self.base))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("{}", failure(status, &text));
        }
        Ok(if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).context("the server's answer is not JSON")? })
    }

    pub async fn get(&self, path: &str) -> anyhow::Result<Value> {
        self.json(Method::GET, path, None).await
    }

    /// The raw answer of a request: status, content type and bytes.
    pub async fn raw(&self, method: Method, path: &str, body: Option<Vec<u8>>) -> anyhow::Result<(StatusCode, String, Vec<u8>)> {
        let mut request = self.request(method, path)?;
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body);
        }
        let response = request.send().await.with_context(|| format!("cannot reach {}", self.base))?;
        let status = response.status();
        let kind = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or("").to_owned();
        Ok((status, kind, response.bytes().await?.to_vec()))
    }

    /// The bytes of a successful download.
    pub async fn download(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let (status, _, bytes) = self.raw(Method::GET, path, None).await?;
        if !status.is_success() {
            bail!("{}", failure(status, &String::from_utf8_lossy(&bytes)));
        }
        Ok(bytes)
    }
}

/// Check the spelling before the URL parser can discard controls, credentials
/// or dot segments. Literal percent signs remain valid path characters.
fn validate_url_text(text: &str) -> anyhow::Result<()> {
    if text.trim() != text || text.chars().any(char::is_control) {
        bail!("invalid API destination");
    }
    let (_, authority_and_path) = text.split_once("://").ok_or_else(|| anyhow!("expected an HTTP(S) URL"))?;
    let authority_end = authority_and_path.find(['/', '?', '#']).unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        bail!("URL credentials or ambiguous authorities are not allowed");
    }
    let rest = &authority_and_path[authority_end..];
    validate_path(rest.split(['?', '#']).next().unwrap_or_default())
}

fn validate_http_url(url: &Url) -> anyhow::Result<()> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("expected an HTTP(S) URL without credentials or fragments");
    }
    validate_path(url.path())
}

/// Canonicalize percent-escape case only, preserving encoded separators.
fn percent_case(path: &str) -> Vec<u8> {
    let mut bytes = path.as_bytes().to_vec();
    for index in 0..bytes.len().saturating_sub(2) {
        if bytes[index] == b'%' && hex_digit(bytes[index + 1]).is_some() && hex_digit(bytes[index + 2]).is_some() {
            bytes[index + 1].make_ascii_uppercase();
            bytes[index + 2].make_ascii_uppercase();
        }
    }
    bytes
}

/// Conservative traversal rejection, not URL canonicalization or a guarantee
/// about arbitrary proxy decoders. Check raw controls/backslashes once, then
/// inspect structure under at most eight percent-decoding layers. Later decoded
/// bytes may be literal percent names, so do not revalidate their text encoding.
fn validate_path(path: &str) -> anyhow::Result<()> {
    if path.contains('\\') || path.chars().any(char::is_control) {
        bail!("invalid API path");
    }
    let mut bytes = path.as_bytes().to_vec();
    for _ in 0..=8 {
        if bytes.split(|byte| matches!(byte, b'/' | b'\\')).any(|part| part == b"." || part == b"..") {
            bail!("ambiguous API path traversal is not allowed");
        }
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%'
                && index + 2 < bytes.len()
                && let (Some(high), Some(low)) = (hex_digit(bytes[index + 1]), hex_digit(bytes[index + 2]))
            {
                decoded.push(high * 16 + low);
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        if decoded == bytes {
            return Ok(());
        }
        bytes = decoded;
    }
    bail!("API path escaping is too deeply nested")
}
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// What a refused request says: the server's `message` when it sent one.
fn failure(status: StatusCode, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body).ok().and_then(|value| value["message"].as_str().map(str::to_owned));
    match (status, message) {
        (StatusCode::UNAUTHORIZED, _) => "not signed in, or the token is not valid: run `rbit auth login`".to_owned(),
        (_, Some(message)) => format!("{message} ({})", status.as_u16()),
        _ => format!("the server answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_name_their_instance_and_repository() {
        let repo = Repo { owner: "alice".into(), name: "rocket".into() };
        for (url, base) in [
            ("https://git.example.com/alice/rocket.git", "https://git.example.com"),
            ("https://git.example.com/alice/rocket", "https://git.example.com"),
            ("http://localhost:3000/alice/rocket.git", "http://localhost:3000"),
            ("https://token@git.example.com/alice/rocket.git", "https://git.example.com"),
            ("ssh://git@git.example.com:2222/alice/rocket.git", "https://git.example.com"),
            ("git@git.example.com:alice/rocket.git", "https://git.example.com"),
            ("https://example.com/git/alice/rocket.git", "https://example.com/git"),
        ] {
            assert_eq!(parse_remote(url), Some((base.to_owned(), repo.clone())), "{url}");
        }
        for url in ["file:///srv/rocket.git", "https://example.com/rocket", "nonsense", "https://example.com/a/../b"] {
            assert_eq!(parse_remote(url), None, "{url}");
        }
        assert!(Repo::parse("alice").is_err() && Repo::parse("a/b c").is_err());
    }

    fn api(base: &str, token: Option<&str>) -> Api {
        rbit_services::webhooks::delivery::install_crypto_provider();
        Api::new(base, token.map(str::to_owned)).unwrap()
    }

    #[test]
    fn requests_stay_on_the_parsed_origin_and_deployment_path() {
        let api = api("https://EXAMPLE.com:443/git/", Some("test-token"));
        for (input, expected) in [
            ("user", "https://example.com/git/api/v1/user"),
            ("/user", "https://example.com/git/api/v1/user"),
            ("HTTPS://example.COM/git/api/v1/user?per_page=1", "https://example.com/git/api/v1/user?per_page=1"),
            ("repos/a/b/branches/feature%2Ftopic", "https://example.com/git/api/v1/repos/a/b/branches/feature%2Ftopic"),
            ("repos/a/b/contents/%E6%97%A5%E6%9C%AC%20file", "https://example.com/git/api/v1/repos/a/b/contents/%E6%97%A5%E6%9C%AC%20file"),
            ("repos/a/b/contents/100%25-complete", "https://example.com/git/api/v1/repos/a/b/contents/100%25-complete"),
            ("repos/a/b/contents/100%-complete", "https://example.com/git/api/v1/repos/a/b/contents/100%-complete"),
            ("contents/100%25bad", "https://example.com/git/api/v1/contents/100%25bad"),
            ("contents/file%2502.txt", "https://example.com/git/api/v1/contents/file%2502.txt"),
            ("contents/file%25ff.txt", "https://example.com/git/api/v1/contents/file%25ff.txt"),
            ("contents/literal%255Cname", "https://example.com/git/api/v1/contents/literal%255Cname"),
            ("https://example.com/git/api/v1/contents/100%25bad", "https://example.com/git/api/v1/contents/100%25bad"),
            ("https://example.com/git/api/v1/contents/file%2502.txt", "https://example.com/git/api/v1/contents/file%2502.txt"),
            ("https://example.com/git/api/v1/contents/file%25ff.txt", "https://example.com/git/api/v1/contents/file%25ff.txt"),
            ("https://example.com/git/api/v1/contents/literal%255Cname", "https://example.com/git/api/v1/contents/literal%255Cname"),
            ("repos/a/b/branches/literal%252Ftext", "https://example.com/git/api/v1/repos/a/b/branches/literal%252Ftext"),
            ("user?q=../%2f%5c%00", "https://example.com/git/api/v1/user?q=../%2f%5c%00"),
        ] {
            let request = api.request(Method::GET, input).unwrap().build().unwrap();
            assert_eq!(request.url().as_str(), expected, "{input}");
            assert_eq!(request.headers()["authorization"], "Bearer test-token");
        }
        for input in [
            "https://example.com.evil/git/api/v1/user",
            "https://example.com:444/git/api/v1/user",
            "http://example.com/git/api/v1/user",
            "https://example.com/git-other/api/v1/user",
            "https://example.com/api/v1/user",
            "https://example.com/git/../other",
            "https://example.com/git/%2e%2e/other",
            "https://name:secret@example.com/git/api/v1/user",
            "https://@example.com/git/api/v1/user",
            "//example.com/git/api/v1/user",
            "https:example.com/git/api/v1/user",
            "ftp://example.com/git/api/v1/user",
            "../other",
            "repos/a/./b",
            "repos/a/%2E%2e/b",
            "repos/a/%252e%252e/b",
            "repos/a/%252F..%252F..%252Fother",
            "repos/a/%5c..%5cother",
            "repos/a/%255c..%255cother",
            "repos/a/\\other",
            "user#fragment",
            "user\n?x=1",
        ] {
            assert!(api.request(Method::GET, input).is_err(), "accepted {input:?}");
        }
        let deeply_escaped = format!("repos/a/%{}2e", "25".repeat(20));
        assert!(api.request(Method::GET, &deeply_escaped).is_err());
    }

    #[test]
    fn instance_urls_are_validated_without_echoing_credentials() {
        rbit_services::webhooks::delivery::install_crypto_provider();
        for base in [
            "ftp://example.com",
            "https://name:secret@example.com/git",
            "https://@example.com/git",
            "https://example.com/git?token=secret",
            "https://example.com/git#fragment",
            "https://example.com/git/../other",
            "https://example.com/git/%252e%252e/other",
            " https://example.com/git",
            "https://example.com\\@evil.test/git",
        ] {
            let result = Api::new(base, None);
            assert!(result.is_err(), "accepted {base:?}");
            assert!(!result.err().unwrap().to_string().contains("secret"));
        }
        for (base, target) in [
            ("http://[::1]:3030/git", "http://[::1]:3030/git/api/v1/user"),
            ("https://example.com/my%20git", "https://example.com/my%20git/api/v1/user"),
            ("https://example.com/my%2fgit", "https://example.com/my%2Fgit/api/v1/user"),
            ("https://example.com/my%2Fgit", "https://example.com/my%2fgit/api/v1/user"),
            ("https://example.com/r%C3%A9bit", "https://example.com/r%c3%a9bit/api/v1/user"),
            ("https://example.com/日本", "https://example.com/%E6%97%A5%E6%9C%AC/api/v1/user"),
        ] {
            assert!(api(base, None).request(Method::GET, target).is_ok());
        }
        assert!(api("https://example.com/my%2fgit", None).request(Method::GET, "https://example.com/my/git/api/v1/user").is_err());
        assert!(api("https://example.com/my%2fgit", None).request(Method::GET, "https://example.com/my%2Fgit-other/api/v1/user").is_err());
    }

    #[derive(Debug)]
    struct CapturedRequest {
        authorization: Option<String>,
        body: Vec<u8>,
    }
    struct TestServer(tokio::task::JoinHandle<()>);
    impl Drop for TestServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    async fn capture_server(
        status: StatusCode,
        destination: Option<String>,
    ) -> (String, tokio::sync::mpsc::Receiver<CapturedRequest>, TestServer) {
        use axum::{
            body::{Body, to_bytes},
            http::{HeaderMap, header},
            response::IntoResponse,
            routing::any,
        };
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        let destination = destination.map(|value| if value == "self" { format!("{base}/api/v1/captured") } else { value });
        let app = axum::Router::new().fallback(any(move |uri: axum::http::Uri, headers: HeaderMap, body: Body| {
            let sender = sender.clone();
            let destination = destination.clone();
            async move {
                let body = to_bytes(body, 4096).await.unwrap().to_vec();
                let authorization = headers.get(header::AUTHORIZATION).map(|value| value.to_str().unwrap().to_owned());
                sender.try_send(CapturedRequest { authorization, body }).unwrap();
                if uri.path() == "/api/v1/captured" {
                    return (StatusCode::OK, "{}").into_response();
                }
                let mut response = (status, "{}").into_response();
                if let Some(destination) = destination {
                    response.headers_mut().insert(header::LOCATION, destination.parse().unwrap());
                }
                response
            }
        }));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (base, receiver, TestServer(task))
    }

    #[tokio::test]
    async fn redirects_never_forward_authenticated_or_unauthenticated_bodies() {
        for token in [None, Some("test-token")] {
            for status in [
                StatusCode::MOVED_PERMANENTLY,
                StatusCode::FOUND,
                StatusCode::SEE_OTHER,
                StatusCode::TEMPORARY_REDIRECT,
                StatusCode::PERMANENT_REDIRECT,
            ] {
                let (destination, mut received_destination, _destination_server) = capture_server(StatusCode::OK, None).await;
                let (origin, mut received_origin, _origin_server) =
                    capture_server(status, Some(format!("{destination}/api/v1/captured"))).await;
                let api = api(&origin, token);
                let bytes = b"private login or package bytes".to_vec();
                let (actual, _, _) = api.raw(Method::POST, "upload", Some(bytes.clone())).await.unwrap();
                assert_eq!(actual, status);
                let captured = received_origin.recv().await.unwrap();
                assert_eq!(captured.body, bytes);
                assert_eq!(captured.authorization, token.map(|value| format!("Bearer {value}")));
                assert!(tokio::time::timeout(Duration::from_millis(30), received_destination.recv()).await.is_err());
            }
        }
    }

    #[tokio::test]
    async fn same_origin_redirects_are_also_visible_instead_of_replayed() {
        let (origin, mut captured, _server) = capture_server(StatusCode::TEMPORARY_REDIRECT, Some("self".to_owned())).await;
        let api = api(&origin, Some("test-token"));
        let body = b"do not replay".to_vec();
        let (status, _, _) = api.raw(Method::POST, "upload", Some(body.clone())).await.unwrap();
        assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(captured.recv().await.unwrap().body, body);
        assert!(tokio::time::timeout(Duration::from_millis(30), captured.recv()).await.is_err());
    }
}
