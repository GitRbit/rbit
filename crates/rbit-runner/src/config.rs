//! `runner.toml` and `.credential` (§12.5.1 of `docs/design/actions.md`).
//!
//! `rbit-runner register` writes both; every other command reads them. The file is strict:
//! an unknown key is an error, because a misspelt `pids_limit` must not silently lift a
//! limit. Relative paths are relative to the directory of `runner.toml`, which is also
//! where the credential lives, outside every task workspace.
//!
//! Each label maps to exactly one backend, so the host backend, which gives jobs the
//! runner user's full access, is never chosen implicitly; [`host_warning`] is printed at
//! registration and at every start.

use std::{
    collections::BTreeSet,
    fs,
    io::Write as _,
    net::IpAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use rbit_workflow::{
    protocol::{self, Backend, Capabilities, DeclaredLabel, MAX_CAPACITY},
    secret::Secret,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};

/// The credential's file name, next to `runner.toml`.
pub const CREDENTIAL_FILE: &str = ".credential";
/// The label `register` creates when none is given.
pub const DEFAULT_LABEL: &str = "ubuntu-latest:container:docker.io/library/ubuntu:24.04";

/// The contents of `runner.toml`. Fields are in file order: plain values first, then the
/// tables (TOML requires it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerConfig {
    /// The rbit instance, `https://…`.
    pub url: String,
    /// Allows an `http://` URL on a loopback host (`register --insecure-http`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub insecure_http: bool,
    pub name: String,
    /// Tasks at once.
    #[serde(default = "default_capacity")]
    pub capacity: u32,
    /// Task workspaces and the cache.
    #[serde(default = "default_work_dir")]
    pub work_dir: PathBuf,
    /// How long running tasks may finish after a shutdown signal before they are
    /// cancelled.
    #[serde(default = "default_shutdown_grace")]
    pub shutdown_grace_seconds: u64,
    /// `runs-on` labels and how their jobs run.
    pub labels: Vec<Label>,
    #[serde(default)]
    pub container: ContainerConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    // Actions: third-party actions.
    #[serde(default)]
    pub actions: ActionsConfig,
}

// Actions: third-party actions.
/// `[actions]`: where the actions jobs name come from, which may run, and the Node.js
/// JavaScript actions run with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsConfig {
    /// The server `owner/repo@ref` actions are fetched from with `git`. Empty (the default)
    /// means this runner's own rbit instance, so nothing is fetched from elsewhere: mirror
    /// the actions workflows use (`actions/setup-node`) there. `https://github.com` fetches
    /// them from GitHub. Only `https://` (or `http://` on a loopback host, and `file://`
    /// for tests).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// The actions jobs may use, as `owner/repo` patterns where `*` matches anything
    /// (`actions/*`, `my-org/*`), or `docker://` image patterns. `["*"]`, the default, allows
    /// every action; `[]` allows only the built-ins and the workspace's own (`./`) actions.
    #[serde(default = "default_allow")]
    pub allow: Vec<String>,
    /// The `node` program JavaScript actions run with on the host. Unset, the runner
    /// downloads a pinned Node.js into its work directory on first use (checked against its
    /// published SHA-256) and uses that, also inside job containers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<PathBuf>,
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self { url: String::new(), allow: default_allow(), node: None }
    }
}

/// One `[[labels]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub name: String,
    pub backend: Backend,
    /// The job container's image, unless the workflow names one (container labels only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// `[container]`: how the container backend runs jobs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerConfig {
    /// `docker` or `podman`, or a path to either.
    #[serde(default = "default_program")]
    pub program: String,
    /// `--memory` of job containers (`4g`); none means no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// `--cpus` of job containers; none means no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,
    /// `--pids-limit` of job containers.
    #[serde(default = "default_pids_limit")]
    pub pids_limit: u32,
    /// The driver of the per-task networks (`bridge`). Never `host`.
    #[serde(default = "default_network")]
    pub network: String,
    /// The image that hands a task's files back to the runner's user (`chown`), never the
    /// job's: it needs `sh`, `chown`, `find` and `chmod`. Pinned by digest; point it at a
    /// mirror on a runner without access to Docker Hub.
    #[serde(default = "default_helper_image", skip_serializing_if = "is_default_helper_image")]
    pub helper_image: String,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            program: default_program(),
            memory: None,
            cpus: None,
            pids_limit: default_pids_limit(),
            network: default_network(),
            helper_image: default_helper_image(),
        }
    }
}

/// `[cache]`: the runner-local cache of §12.5.6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Least-recently-used entries are evicted beyond this size.
    #[serde(default = "default_cache_bytes")]
    pub max_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { max_bytes: default_cache_bytes() }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's `skip_serializing_if` passes a reference
fn is_false(value: &bool) -> bool {
    !value
}
fn default_capacity() -> u32 {
    1
}
fn default_work_dir() -> PathBuf {
    PathBuf::from("work")
}
fn default_shutdown_grace() -> u64 {
    600
}
fn default_program() -> String {
    "docker".to_owned()
}
fn default_pids_limit() -> u32 {
    4096
}
fn default_network() -> String {
    "bridge".to_owned()
}
fn default_helper_image() -> String {
    crate::backend::container::HELPER_IMAGE.to_owned()
}
fn is_default_helper_image(image: &String) -> bool {
    image == crate::backend::container::HELPER_IMAGE
}
fn default_allow() -> Vec<String> {
    vec!["*".to_owned()]
}
fn default_cache_bytes() -> u64 {
    10 * 1024 * 1024 * 1024
}

impl RunnerConfig {
    /// Checks everything the server would refuse, and what this runner refuses itself.
    pub fn validate(&self) -> anyhow::Result<()> {
        server_url(&self.url, self.insecure_http)?;
        ensure!((1..=MAX_CAPACITY).contains(&self.capacity), "`capacity` must be between 1 and {MAX_CAPACITY}");
        for label in &self.labels {
            match (label.backend, &label.image) {
                (Backend::Container, None) => bail!("container label `{}` needs an `image`", label.name),
                (Backend::Container, Some(image)) if image.trim().is_empty() || image.starts_with('-') => {
                    bail!("label `{}` has a malformed image", label.name)
                }
                (Backend::Host, Some(_)) => bail!("host label `{}` cannot have an image", label.name),
                _ => {}
            }
        }
        let program = self.container.program.trim();
        ensure!(!program.is_empty() && !program.starts_with('-'), "`container.program` must name docker or podman");
        ensure!(self.container.pids_limit >= 1, "`container.pids_limit` must be at least 1");
        if let Some(cpus) = self.container.cpus {
            ensure!(cpus.is_finite() && cpus > 0.0, "`container.cpus` must be a positive number");
        }
        if let Some(memory) = &self.container.memory {
            ensure!(valid_memory(memory), "`container.memory` must be a size such as `4g` or `512m`");
        }
        let network = self.container.network.as_str();
        ensure!(!network.eq_ignore_ascii_case("host"), "`container.network` cannot be `host`: jobs never share the host's network");
        ensure!(
            !network.is_empty() && network.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
            "`container.network` must be a network driver name such as `bridge`"
        );
        ensure!(self.cache.max_bytes > 0, "`cache.max_bytes` must be positive");
        // Actions: third-party actions. Empty: the runner's own instance, checked above.
        if !self.actions.url.trim().is_empty() {
            let actions = Url::parse(&self.actions.url).with_context(|| format!("`actions.url` `{}` is not a URL", self.actions.url))?;
            let loopback = actions.host_str().is_some_and(|host| {
                host == "localhost" || host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
            });
            ensure!(
                actions.scheme() == "https" || actions.scheme() == "file" || (actions.scheme() == "http" && loopback),
                "`actions.url` must be `https://` (or `http://` on this machine)"
            );
        }
        for pattern in &self.actions.allow {
            ensure!(!pattern.is_empty() && !pattern.chars().any(char::is_whitespace), "`actions.allow` has an empty or spaced pattern");
        }
        protocol::DeclareRequest { name: self.name.clone(), labels: self.declared_labels(), capabilities: self.capabilities(false) }
            .validate()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        Ok(())
    }

    pub fn declared_labels(&self) -> Vec<DeclaredLabel> {
        self.labels.iter().map(|label| DeclaredLabel { name: label.name.clone(), backend: label.backend }).collect()
    }

    /// The backends the labels use, each once.
    pub fn backends(&self) -> Vec<Backend> {
        self.labels.iter().map(|label| label.backend).collect::<BTreeSet<_>>().into_iter().collect()
    }

    pub fn capabilities(&self, ephemeral: bool) -> Capabilities {
        Capabilities {
            backends: self.backends(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            capacity: if ephemeral { 1 } else { self.capacity },
            version: env!("CARGO_PKG_VERSION").to_owned(),
            ephemeral,
        }
    }

    pub fn host_labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter().filter(|label| label.backend == Backend::Host).map(|label| label.name.as_str())
    }

    /// The label a task's `runs-on` selected: the first of its labels this runner serves.
    pub fn label_for<'a>(&self, runs_on: impl IntoIterator<Item = &'a String>) -> Option<&Label> {
        runs_on.into_iter().find_map(|wanted| self.labels.iter().find(|label| &label.name == wanted))
    }
}

/// `4g`, `512m`, `1024k` or plain bytes, as `docker run --memory` takes them.
fn valid_memory(value: &str) -> bool {
    let digits = value.strip_suffix(['b', 'k', 'm', 'g', 'B', 'K', 'M', 'G']).unwrap_or(value);
    !digits.is_empty() && digits.len() <= 18 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Parses `name:backend[:image]` (`register --labels`). The image keeps its own colons.
pub fn parse_label(spec: &str) -> anyhow::Result<Label> {
    let mut parts = spec.splitn(3, ':');
    let name = parts.next().unwrap_or_default().trim();
    let backend = match parts.next().map(str::trim) {
        Some("container") => Backend::Container,
        Some("host") => Backend::Host,
        Some(other) => bail!("label `{spec}`: the backend must be `container` or `host`, not `{other}`"),
        None => bail!("label `{spec}` must be `name:backend[:image]`"),
    };
    let image = parts.next().map(|image| image.trim().to_owned()).filter(|image| !image.is_empty());
    Ok(Label { name: name.to_owned(), backend, image })
}

/// The server's base URL: `https`, or `http` on a loopback host when `insecure_http` is
/// set; no credentials, query or fragment; any trailing `/` dropped.
pub fn server_url(url: &str, insecure_http: bool) -> anyhow::Result<Url> {
    let parsed = Url::parse(url).with_context(|| format!("`{url}` is not a URL"))?;
    match parsed.scheme() {
        "https" => {}
        "http" if insecure_http && is_loopback(&parsed) => {}
        "http" if insecure_http => bail!("`--insecure-http` is only allowed for a server on this machine (localhost)"),
        "http" => bail!("the server URL must use https; pass `--insecure-http` for a server on this machine"),
        scheme => bail!("the server URL must use https, not `{scheme}`"),
    }
    ensure!(parsed.host_str().is_some_and(|host| !host.is_empty()), "the server URL needs a host");
    ensure!(parsed.username().is_empty() && parsed.password().is_none(), "the server URL must not carry credentials");
    ensure!(parsed.query().is_none() && parsed.fragment().is_none(), "the server URL must not have a query or fragment");
    let mut base = parsed;
    let path = base.path().trim_end_matches('/').to_owned();
    base.set_path(&path);
    Ok(base)
}

fn is_loopback(url: &Url) -> bool {
    match url.host_str() {
        Some("localhost") => true,
        Some(host) => host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()),
        None => false,
    }
}

/// The warning for host labels, or `None` without any.
pub fn host_warning(config: &RunnerConfig) -> Option<String> {
    let labels: Vec<_> = config.host_labels().map(|name| format!("`{name}`")).collect();
    (!labels.is_empty()).then(|| {
        format!(
            "WARNING: the label(s) {} run jobs on the HOST backend, with no isolation. Job code runs as this \
             user and can read and change everything it can, including the runner credential. Use host labels \
             only on repository-scoped runners of trusted repositories.",
            labels.join(", ")
        )
    })
}

/// A configuration read from disk, with its paths resolved.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: RunnerConfig,
    /// `work_dir`, made absolute against the configuration's directory.
    pub work_dir: PathBuf,
    pub credential_path: PathBuf,
}

/// The directory `runner.toml` is in (`.` for a bare file name).
pub fn config_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

pub fn credential_path(config_path: &Path) -> PathBuf {
    config_dir(config_path).join(CREDENTIAL_FILE)
}

/// Reads and validates `runner.toml`.
pub fn load(path: &Path) -> anyhow::Result<Loaded> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read {}; register the runner first with `rbit-runner register`", path.display()))?;
    let config: RunnerConfig = toml::from_str(&text).with_context(|| format!("{} is not a valid runner configuration", path.display()))?;
    config.validate().with_context(|| format!("{} is not a valid runner configuration", path.display()))?;
    let dir = config_dir(path);
    let work_dir = if config.work_dir.is_absolute() { config.work_dir.clone() } else { dir.join(&config.work_dir) };
    let work_dir = std::path::absolute(&work_dir).unwrap_or(work_dir);
    Ok(Loaded { config, work_dir, credential_path: credential_path(path) })
}

/// Writes `runner.toml` through a temporary file, so a crash never leaves half a file.
pub fn save(path: &Path, config: &RunnerConfig) -> anyhow::Result<()> {
    let body = toml::to_string_pretty(config).context("cannot write the runner configuration")?;
    let text = format!(
        "# rbit-runner configuration, written by `rbit-runner register`. See docs/deployment.md.\n\
         # The runner credential is in {CREDENTIAL_FILE} next to this file; keep it private.\n\n{body}"
    );
    write_atomically(path, text.as_bytes(), false)
}

/// Reads the runner credential and checks its shape.
pub fn read_credential(path: &Path) -> anyhow::Result<Secret> {
    let credential = Secret::new(
        fs::read_to_string(path).with_context(|| format!("cannot read the runner credential at {}", path.display()))?.trim().to_owned(),
    );
    ensure!(protocol::valid_runner_credential(credential.expose()), "{} does not hold a runner credential", path.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            tracing::warn!(path = %path.display(), mode = format!("{:o}", mode & 0o777), "the runner credential is readable by other users; chmod 600 it");
        }
    }
    Ok(credential)
}

/// Writes the credential, readable by the owner only on Unix. On Windows the directory's
/// ACLs decide; the deployment docs say to restrict them.
pub fn write_credential(path: &Path, credential: &Secret) -> anyhow::Result<()> {
    write_atomically(path, format!("{}\n", credential.expose()).as_bytes(), true)
}

fn write_atomically(path: &Path, contents: &[u8], private: bool) -> anyhow::Result<()> {
    let dir = config_dir(path);
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let name = path.file_name().map_or_else(|| "runner".into(), |name| name.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.tmp"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options.open(&temp).with_context(|| format!("cannot write {}", temp.display()))?;
    file.write_all(contents).and_then(|()| file.sync_all()).with_context(|| format!("cannot write {}", temp.display()))?;
    drop(file);
    fs::rename(&temp, path).with_context(|| format!("cannot write {}", path.display()))
}

/// This host's name, for a runner registered without `--name`.
pub fn host_name() -> String {
    let from_env = ["HOSTNAME", "COMPUTERNAME"].iter().find_map(|name| std::env::var(name).ok());
    let name = from_env.or_else(|| fs::read_to_string("/etc/hostname").ok()).unwrap_or_default();
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(protocol::MAX_RUNNER_NAME_CHARS).collect();
    if name.is_empty() { "rbit-runner".to_owned() } else { name }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A change that makes a configuration invalid, and part of the message it is refused
    /// with.
    type Refusal = (fn(&mut RunnerConfig), &'static str);

    pub(crate) fn config() -> RunnerConfig {
        RunnerConfig {
            url: "https://git.example.com".into(),
            insecure_http: false,
            name: "build-1".into(),
            capacity: 2,
            work_dir: PathBuf::from("work"),
            shutdown_grace_seconds: 600,
            labels: vec![parse_label(DEFAULT_LABEL).unwrap(), parse_label("self-hosted:host").unwrap()],
            container: ContainerConfig::default(),
            cache: CacheConfig::default(),
            actions: ActionsConfig::default(),
        }
    }

    #[test]
    fn the_design_example_parses() {
        let text = r#"
url = "https://git.example.com"
name = "build-1"
capacity = 2                       # tasks at once
work_dir = "/var/lib/rbit-runner"
[[labels]]                         # runs-on label → how to run it
name = "ubuntu-latest"
backend = "container"
image = "docker.io/library/ubuntu:24.04"
[[labels]]
name = "self-hosted"
backend = "host"                   # warned about at register and every start
[container]
program = "docker"                 # or "podman"
memory = "4g"
cpus = 2
pids_limit = 4096
network = "bridge"                 # per-job networks are created on top
[cache]
max_bytes = 10737418240
"#;
        let parsed: RunnerConfig = toml::from_str(text).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed.labels, config().labels);
        assert_eq!((parsed.container.memory.as_deref(), parsed.container.cpus), (Some("4g"), Some(2.0)));
        assert_eq!(parsed.backends(), [Backend::Container, Backend::Host]);
        assert_eq!(parsed.cache.max_bytes, 10 * 1024 * 1024 * 1024);
        let warning = host_warning(&parsed).unwrap();
        assert!(warning.contains("`self-hosted`") && warning.contains("no isolation") && warning.contains("runner credential"));
    }

    #[test]
    fn the_shipped_example_is_valid() {
        let example: RunnerConfig = toml::from_str(include_str!("../../../deploy/rbit-runner.example.toml")).unwrap();
        example.validate().unwrap();
        assert_eq!((example.capacity, example.shutdown_grace_seconds), (2, 600));
        assert!(host_warning(&example).is_none(), "the example's host label stays commented out");
    }

    #[test]
    fn unknown_keys_and_unsafe_settings_are_refused() {
        assert!(toml::from_str::<RunnerConfig>("url='https://a'\nname='n'\nlabels=[]\n[container]\npid_limit=1\n").is_err());
        let refused: [Refusal; 9] = [
            (|c| c.capacity = 0, "capacity"),
            (|c| c.labels[0].image = None, "needs an `image`"),
            (|c| c.labels[1].image = Some("x".into()), "cannot have an image"),
            (|c| c.labels.clear(), "labels"),
            (|c| c.container.network = "HOST".into(), "cannot be `host`"),
            (|c| c.container.memory = Some("lots".into()), "`container.memory`"),
            (|c| c.container.cpus = Some(0.0), "`container.cpus`"),
            (|c| c.container.program = "--privileged".into(), "`container.program`"),
            (|c| c.url = "http://git.example.com".into(), "https"),
        ];
        for (change, message) in refused {
            let mut config = config();
            change(&mut config);
            let error = format!("{:#}", config.validate().unwrap_err());
            assert!(error.contains(message), "{error:?} should mention {message:?}");
        }
    }

    #[test]
    fn server_urls_need_tls_except_on_loopback() {
        assert_eq!(server_url("https://git.example.com/", false).unwrap().as_str(), "https://git.example.com/");
        assert_eq!(server_url("https://example.com/rbit/", false).unwrap().as_str(), "https://example.com/rbit");
        for url in ["http://localhost:3000", "http://127.0.0.1:3000", "http://[::1]:3000"] {
            assert!(server_url(url, false).is_err(), "{url}");
            server_url(url, true).unwrap();
        }
        let refused = [
            ("http://git.example.com", true, "only allowed"),
            ("ftp://git.example.com", false, "not `ftp`"),
            ("https://user:pw@git.example.com", false, "credentials"),
            ("https://git.example.com/?x=1", false, "query"),
            ("not a url", false, "not a URL"),
        ];
        for (url, insecure, message) in refused {
            let error = server_url(url, insecure).unwrap_err().to_string();
            assert!(error.contains(message), "{url}: {error}");
        }
    }

    #[test]
    fn labels_parse_with_images_that_have_colons() {
        assert_eq!(
            parse_label("ubuntu-latest:container:docker.io/library/ubuntu:24.04").unwrap(),
            Label { name: "ubuntu-latest".into(), backend: Backend::Container, image: Some("docker.io/library/ubuntu:24.04".into()) }
        );
        assert_eq!(parse_label("self-hosted:host").unwrap(), Label { name: "self-hosted".into(), backend: Backend::Host, image: None });
        assert!(parse_label("self-hosted").is_err());
        assert!(parse_label("x:vm").is_err());
    }

    #[test]
    fn round_trips_through_disk_and_keeps_the_credential_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runner.toml");
        save(&path, &config()).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.config, config());
        assert_eq!(loaded.work_dir, std::path::absolute(dir.path().join("work")).unwrap());
        assert_eq!(loaded.credential_path, dir.path().join(CREDENTIAL_FILE));

        let credential = Secret::new(format!("rbrn_{}", "A".repeat(43)));
        write_credential(&loaded.credential_path, &credential).unwrap();
        assert_eq!(read_credential(&loaded.credential_path).unwrap(), credential);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(fs::metadata(&loaded.credential_path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        fs::write(&loaded.credential_path, "rbrn_short").unwrap();
        assert!(read_credential(&loaded.credential_path).is_err());
        assert!(load(&dir.path().join("missing.toml")).unwrap_err().to_string().contains("register the runner first"));
    }

    #[test]
    fn labels_are_matched_in_runs_on_order() {
        let config = config();
        let runs_on = vec!["self-hosted".to_owned(), "ubuntu-latest".to_owned()];
        assert_eq!(config.label_for(&runs_on).unwrap().name, "self-hosted");
        assert!(config.label_for(&vec!["windows-latest".to_owned()]).is_none());
    }

    #[test]
    fn ephemeral_runners_take_one_task() {
        let config = config();
        assert_eq!((config.capabilities(false).capacity, config.capabilities(true).capacity), (2, 1));
        assert!(config.capabilities(true).ephemeral);
        assert_ne!(host_name(), "");
    }
}
