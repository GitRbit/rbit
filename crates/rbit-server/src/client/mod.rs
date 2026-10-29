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
use reqwest::{Method, StatusCode};
use serde_json::Value;

pub use commands::{ClientCommand, run};

/// An authenticated connection to one instance's REST API.
pub struct Api {
    http: reqwest::Client,
    /// `https://git.example.com`, without a trailing slash.
    base: String,
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
        let http = reqwest::Client::builder()
            .user_agent(concat!("rbit-cli/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()
            .context("cannot set up HTTPS")?;
        Ok(Self { http, base: base.trim_end_matches('/').to_owned(), token })
    }

    /// `path` (below `/api/v1`, or a full URL of this instance) as a request.
    fn request(&self, method: Method, path: &str) -> anyhow::Result<reqwest::RequestBuilder> {
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            if !path.starts_with(&self.base) {
                bail!("`{path}` is not on {}", self.base);
            }
            path.to_owned()
        } else {
            format!("{}/api/v1/{}", self.base, path.trim_start_matches('/'))
        };
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
}
