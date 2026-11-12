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

mod agent;
mod bisect;
mod ci;
mod commands;
mod failure;
mod git_credential;
mod hosts;
mod issues;
mod pulls;
mod runs;
mod runs_view;
mod status;
mod task;
mod util;

use std::time::Duration;

use anyhow::{Context as _, anyhow, bail};
use reqwest::{Method, StatusCode};
use serde_json::Value;

pub use agent::exit_code;
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

/// A Git remote: its name, and its instance and repository when its URL names them.
pub type Remote = (String, Option<(String, Repo)>);

/// The remote [`choose_remote`] picked (its name, instance and repository), or why none.
pub type Chosen<'a> = Result<Option<(&'a str, &'a (String, Repo))>, String>;

/// The remote that names the repository: `named` when given; otherwise `origin` when it is on
/// a known instance (one `known` accepts), else the one remote on a known instance. With no
/// instance known at all (`any_known` false, a token in the environment only), `origin`.
/// `Err` when instances are known but no remote is on one, or several are, since sending
/// another host's path (`origin` on GitHub, say) to an rbit instance only finds nothing.
pub fn choose_remote<'a>(named: Option<&str>, remotes: &'a [Remote], known: &dyn Fn(&str) -> bool, any_known: bool) -> Chosen<'a> {
    let parsed = |remote: &'a Remote| remote.1.as_ref().map(|found| (remote.0.as_str(), found));
    if let Some(name) = named {
        return Ok(remotes.iter().find(|remote| remote.0 == name).and_then(parsed));
    }
    let origin = remotes.iter().find(|remote| remote.0 == "origin").and_then(parsed);
    if !any_known || origin.is_some_and(|(_, (host, _))| known(host)) {
        return Ok(origin);
    }
    let on_known: Vec<_> = remotes.iter().filter_map(parsed).filter(|(_, (host, _))| known(host)).collect();
    match (on_known.as_slice(), origin) {
        ([one], _) => Ok(Some(*one)),
        ([], None) => Ok(None),
        ([], Some((_, (host, _)))) => {
            Err(format!("`origin` is on {host}, not on an rbit instance you are logged in to: pass --remote NAME or --repo owner/name"))
        }
        (several, _) => {
            let listed: Vec<&str> = several.iter().map(|(name, _)| *name).collect();
            Err(format!("remotes {} are all on rbit instances you know: pass --remote NAME", listed.join(", ")))
        }
    }
}

/// The token a request to an instance carries: one from the environment (`env`) only when
/// the instance is `trusted` (named with `--hostname`, or one with a stored login), never a
/// host only a Git remote suggested; otherwise the instance's `stored` login. `Err` when an
/// environment token would go to such a host: it may be meant for another instance.
pub fn token_for(env: Option<String>, trusted: bool, host: &str, stored: Option<String>) -> Result<Option<String>, String> {
    match env {
        Some(_) if !trusted => Err(format!(
            "the token in RBIT_TOKEN or GH_TOKEN is not sent to {host}, which only a Git remote names: set RBIT_HOST (or pass --hostname)"
        )),
        Some(token) => Ok(Some(token)),
        None => Ok(stored),
    }
}

/// Times a throttled read is tried again.
const RATE_LIMIT_RETRIES: u32 = 3;
/// The longest `Retry-After` waited for; a longer one is the caller's to decide.
const MAX_RETRY_AFTER_SECONDS: u64 = 30;

/// How long to wait before trying attempt `attempt` (0 for the first) again: a `GET` the
/// server answered `429` with a short `Retry-After`, at most [`RATE_LIMIT_RETRIES`] times.
/// Writes are never repeated.
fn retry_wait(status: StatusCode, method: &Method, retry_after: Option<&str>, attempt: u32) -> Option<Duration> {
    if status != StatusCode::TOO_MANY_REQUESTS || *method != Method::GET || attempt >= RATE_LIMIT_RETRIES {
        return None;
    }
    let seconds = retry_after?.trim().parse::<u64>().ok().filter(|seconds| *seconds <= MAX_RETRY_AFTER_SECONDS)?;
    Some(Duration::from_secs(seconds.max(1)))
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

    /// The JSON answer of a request, or an error with the server's message. A read the
    /// server throttles is tried again after the `Retry-After` it gives (see [`retry_wait`]).
    pub async fn json(&self, method: Method, path: &str, body: Option<&Value>) -> anyhow::Result<Value> {
        let mut attempt = 0;
        let response = loop {
            let mut request = self.request(method.clone(), path)?;
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request.send().await.with_context(|| format!("cannot reach {}", self.base))?;
            let after = response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value| value.to_str().ok());
            match retry_wait(response.status(), &method, after, attempt) {
                Some(wait) => tokio::time::sleep(wait).await,
                None => break response,
            }
            attempt += 1;
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(agent::ApiFailure::from_response(status.as_u16(), &text).into());
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
            return Err(agent::ApiFailure::from_response(status.as_u16(), &String::from_utf8_lossy(&bytes)).into());
        }
        Ok(bytes)
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

    #[test]
    fn an_environment_token_goes_only_to_an_instance_the_user_named() {
        let env = || Some("t".to_owned());
        assert_eq!(token_for(env(), true, "https://git.example.com", None), Ok(env()));
        let refused = token_for(env(), false, "http://127.0.0.1:18080", None).unwrap_err();
        assert!(refused.contains("127.0.0.1:18080") && refused.contains("RBIT_HOST"), "{refused}");
        assert_eq!(token_for(None, false, "https://git.example.com", None), Ok(None), "no token, nothing sent");
        assert_eq!(token_for(None, true, "https://git.example.com", Some("s".to_owned())), Ok(Some("s".to_owned())));
    }

    #[test]
    fn a_throttled_read_waits_and_tries_again_but_a_write_does_not() {
        let limited = StatusCode::TOO_MANY_REQUESTS;
        assert_eq!(retry_wait(limited, &Method::GET, Some("7"), 0), Some(Duration::from_secs(7)));
        assert_eq!(retry_wait(limited, &Method::GET, Some("0"), 2), Some(Duration::from_secs(1)));
        assert_eq!(retry_wait(limited, &Method::GET, Some("7"), RATE_LIMIT_RETRIES), None, "it gives up");
        assert_eq!(retry_wait(limited, &Method::GET, Some("600"), 0), None, "a long wait is the caller's to decide");
        assert_eq!(retry_wait(limited, &Method::GET, None, 0), None);
        assert_eq!(retry_wait(limited, &Method::POST, Some("7"), 0), None, "a write is never repeated");
        assert_eq!(retry_wait(StatusCode::SERVICE_UNAVAILABLE, &Method::GET, Some("7"), 0), None);
    }

    #[test]
    fn the_remote_on_a_known_instance_names_the_repository() {
        fn name(chosen: Chosen<'_>) -> Result<Option<String>, String> {
            chosen.map(|found| found.map(|(name, _)| name.to_owned()))
        }
        let remote = |name: &str, url: &str| (name.to_owned(), parse_remote(url));
        let (github, home) = (remote("origin", "https://github.com/up/rocket"), remote("home", "https://git.example.com/alice/rocket"));
        let known = |host: &str| host == "https://git.example.com";
        let mirror = [github.clone(), home.clone()];
        assert_eq!(name(choose_remote(None, &mirror, &known, true)), Ok(Some("home".to_owned())));
        assert_eq!(name(choose_remote(Some("origin"), &mirror, &known, true)), Ok(Some("origin".to_owned())));
        assert_eq!(name(choose_remote(None, &mirror, &known, false)), Ok(Some("origin".to_owned())), "nothing known: origin");
        let elsewhere = choose_remote(None, std::slice::from_ref(&github), &known, true).unwrap_err();
        assert!(elsewhere.contains("github.com") && elsewhere.contains("--remote"), "{elsewhere}");
        let both = [home.clone(), ("fork".to_owned(), home.1.clone())];
        assert!(choose_remote(None, &both, &known, true).unwrap_err().contains("home, fork"));
        assert_eq!(name(choose_remote(None, &[], &known, true)), Ok(None));
    }
}
