//! The client's subcommands, as close to `gh`'s as rbit's API allows.

use std::{io::Read as _, path::PathBuf};

use anyhow::{Context as _, anyhow, bail};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Map, Value, json};

use super::{
    Api, Repo, agent, hosts, parse_remote, pulls, runs,
    util::{clip, encode, git, split_pair, text, value_text, wildcard},
};

/// Where a command works: the repository and the instance.
#[derive(Debug, Clone, Args)]
pub struct Target {
    /// The repository, `owner/name`; the `origin` remote's when not given.
    #[arg(short = 'R', long, global = true)]
    repo: Option<String>,
    /// The instance, `https://git.example.com`; the remote's, or the one logged in to.
    #[arg(long, env = "RBIT_HOST", global = true)]
    hostname: Option<String>,
    /// The Git remote that names the repository and instance (`origin` by default).
    #[arg(long, env = "RBIT_REMOTE", global = true)]
    remote: Option<String>,
    #[command(flatten)]
    output: Output,
}

/// How a command answers: for people, or one JSON object for programs (`docs/cli.md`).
#[derive(Debug, Clone, Default, Args)]
pub struct Output {
    /// Print one compact JSON object with a `schema`; failures too, with a `code`.
    #[arg(long, global = true)]
    pub json: bool,
    /// With `--json`, only these fields of each listed item (comma-separated).
    #[arg(long, global = true, value_delimiter = ',')]
    pub fields: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum ClientCommand {
    /// Log in to an instance with a personal access token, see the logins, or log out.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Workflow runs: list, view, watch, rerun, cancel and download their artifacts.
    Run {
        #[command(subcommand)]
        command: RunCommand,
        #[command(flatten)]
        target: Target,
    },
    /// Workflows: list them and run one by hand.
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
        #[command(flatten)]
        target: Target,
    },
    /// Artifacts: list and download.
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommand,
        #[command(flatten)]
        target: Target,
    },
    /// Issues: list, view, create, comment, close and label.
    Issue {
        #[command(subcommand)]
        command: super::issues::IssueCommand,
        #[command(flatten)]
        target: Target,
    },
    /// Where a branch stands: its head, its CI for exactly that commit, its base, its pull
    /// request and the next command worth running (the current branch by default).
    Status {
        #[arg(short, long)]
        branch: Option<String>,
        #[command(flatten)]
        target: Target,
    },
    /// Pull requests: create, list, view, merge and their checks.
    Pr {
        #[command(subcommand)]
        command: PrCommand,
        #[command(flatten)]
        target: Target,
    },
    /// A request to any REST API path, e.g. `rbit api repos/{owner}/{repo}/actions/runs`.
    Api {
        /// The path below `/api/v1`; `{owner}` and `{repo}` are the current repository's.
        path: String,
        /// The method; POST when fields are given, GET otherwise.
        #[arg(short = 'X', long)]
        method: Option<String>,
        /// A string field, `key=value` (repeatable).
        #[arg(short = 'f', long = "raw-field")]
        raw_fields: Vec<String>,
        /// A typed field, `key=value`: `true`, `false`, `null` and numbers keep their type (repeatable).
        #[arg(short = 'F', long = "field")]
        fields: Vec<String>,
        /// The request body from a file (`-` for standard input).
        #[arg(long)]
        input: Option<PathBuf>,
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Store a token for an instance, read from standard input or `RBIT_TOKEN`.
    Login {
        /// The instance, `https://git.example.com`.
        #[arg(long)]
        hostname: String,
        /// Read the token from standard input (the default when it is not a terminal).
        #[arg(long)]
        with_token: bool,
    },
    /// The stored logins, each checked against its instance.
    Status,
    /// Forget an instance's login.
    Logout {
        #[arg(long)]
        hostname: String,
    },
    /// Print the token of an instance, for scripts.
    Token {
        #[arg(long)]
        hostname: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum RunCommand {
    /// The newest runs.
    List {
        /// Only runs of this workflow (its file name, `ci.yml`, or its name).
        #[arg(short, long)]
        workflow: Option<String>,
        #[arg(short, long)]
        branch: Option<String>,
        #[arg(short, long)]
        event: Option<String>,
        /// `queued`, `in_progress`, `completed`, `success`, `failure`, …
        #[arg(short, long)]
        status: Option<String>,
        /// Only runs that tested this commit (an id or a prefix of 7 or more hex digits).
        #[arg(long)]
        sha: Option<String>,
        #[arg(short = 'L', long, default_value_t = 20)]
        limit: u32,
    },
    /// A run and its jobs; without an id, every run of `--sha` or of `HEAD`.
    View {
        run: Option<i64>,
        /// The commit whose runs to show, when no run id is given (default: `HEAD`).
        #[arg(long, conflicts_with = "run")]
        sha: Option<String>,
        /// Print every job's log.
        #[arg(long)]
        log: bool,
        /// Print the logs of the jobs that failed.
        #[arg(long)]
        log_failed: bool,
        /// Only this job.
        #[arg(short, long)]
        job: Option<i64>,
        /// Exit with 1 when a run did not succeed, 8 while one has not finished.
        #[arg(long)]
        exit_status: bool,
    },
    /// Follow a run until it finishes; without an id, every run of `--sha` or of `HEAD`,
    /// waiting for the first to appear.
    Watch {
        run: Option<i64>,
        /// The commit whose runs to follow, when no run id is given (default: `HEAD`).
        #[arg(long, conflicts_with = "run")]
        sha: Option<String>,
        /// Exit with 1 when a run did not succeed.
        #[arg(long)]
        exit_status: bool,
        /// Seconds between looks.
        #[arg(short, long, default_value_t = 3)]
        interval: u64,
        /// Give up after this many seconds with `timeout` (exit 8).
        #[arg(long, default_value_t = 3600)]
        timeout: u64,
        /// With a commit: end with `head_changed` (exit 5) when this branch moves on (the
        /// current branch when watching `HEAD`).
        #[arg(long, conflicts_with = "run")]
        branch: Option<String>,
    },
    /// Run a run's jobs again: all, the failed ones, or one.
    Rerun {
        run: i64,
        #[arg(long)]
        failed: bool,
        #[arg(short, long)]
        job: Option<i64>,
    },
    /// Cancel a run.
    Cancel { run: i64 },
    /// What failed: per failed job, how it ended, its error annotations and the last lines
    /// of the step that ended it. Without an id, every failed run of `--sha` or of `HEAD`.
    Failure {
        run: Option<i64>,
        #[arg(long, conflicts_with = "run")]
        sha: Option<String>,
    },
    /// One step's log lines: the last 100 by default.
    Log {
        job: i64,
        /// The step's number, as `run view` lists it.
        #[arg(long)]
        step: i64,
        #[arg(long, conflicts_with = "from")]
        tail: Option<u32>,
        /// Start at this task-log line.
        #[arg(long)]
        from: Option<u64>,
        /// With `--from`, how many lines.
        #[arg(long, requires = "from")]
        lines: Option<u32>,
        /// An earlier attempt of the job (default: the current one).
        #[arg(long)]
        attempt: Option<i64>,
        /// Keep timestamps and colour codes.
        #[arg(long)]
        raw: bool,
    },
    /// Download a run's artifacts; without an id, the run of `--sha` or of `HEAD`.
    Download {
        run: Option<i64>,
        /// The commit whose run to download from, when no run id is given (default: `HEAD`).
        #[arg(long, conflicts_with = "run")]
        sha: Option<String>,
        /// Only artifacts of these names (repeatable).
        #[arg(short, long)]
        name: Vec<String>,
        /// Only artifacts whose name matches this pattern (`*` and `?`).
        #[arg(short, long)]
        pattern: Option<String>,
        /// Where to unpack; one directory per artifact unless one was named.
        #[arg(short = 'D', long, default_value = ".")]
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// The repository's workflows.
    List,
    /// Run a workflow with `workflow_dispatch`.
    Run {
        /// The workflow's file name (`deploy.yml`) or path.
        workflow: String,
        /// The branch or tag; the default branch when not given.
        #[arg(short, long = "ref")]
        reference: Option<String>,
        /// An input, `key=value` (repeatable).
        #[arg(short = 'f', long = "raw-field")]
        inputs: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ArtifactCommand {
    /// The repository's artifacts, or a run's.
    List {
        #[arg(long)]
        run: Option<i64>,
    },
    /// Download one artifact by id, or by name from a run (the newest run with one of that name).
    Download {
        /// The artifact's id or name.
        artifact: String,
        #[arg(long)]
        run: Option<i64>,
        #[arg(short = 'D', long, default_value = ".")]
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum PrCommand {
    /// Open a pull request from a branch.
    Create {
        #[arg(short, long)]
        title: String,
        #[arg(short, long, default_value = "")]
        body: String,
        /// The branch to merge into; the default branch when not given.
        #[arg(short = 'B', long)]
        base: Option<String>,
        /// The branch with the changes; the current branch when not given.
        #[arg(short = 'H', long)]
        head: Option<String>,
        #[arg(short, long)]
        draft: bool,
    },
    /// Open pull requests.
    List {
        #[arg(short, long, default_value = "open")]
        state: String,
        /// Only pull requests from this branch.
        #[arg(short = 'H', long)]
        head: Option<String>,
        #[arg(short = 'L', long, default_value_t = 30)]
        limit: u32,
    },
    /// A pull request.
    View { number: u64 },
    /// Merge a pull request, only if its head is still `--head` (by default the head read
    /// just before merging).
    Merge {
        number: u64,
        #[arg(long, conflicts_with_all = ["squash", "rebase"])]
        merge: bool,
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        #[arg(long)]
        rebase: bool,
        /// The head commit that may be merged; a newer head is refused (`head_changed`).
        #[arg(long)]
        head: Option<String>,
    },
    /// The checks of a pull request's head: exit 0 when all passed, 1 when one failed, 8
    /// while some are pending or none has been reported yet.
    Checks {
        number: u64,
        /// Wait until every check finished.
        #[arg(long)]
        watch: bool,
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
}

/// Runs `command`; the process exit code. With `--json`, a failure is printed as an error
/// object on stdout and its exit code returned.
pub async fn run(command: ClientCommand) -> anyhow::Result<i32> {
    let json = match &command {
        ClientCommand::Run { target, .. }
        | ClientCommand::Workflow { target, .. }
        | ClientCommand::Artifact { target, .. }
        | ClientCommand::Pr { target, .. }
        | ClientCommand::Status { target, .. }
        | ClientCommand::Issue { target, .. }
        | ClientCommand::Api { target, .. } => target.output.json,
        ClientCommand::Auth { .. } => false,
    };
    match dispatch(command).await {
        Err(error) if json => {
            println!("{}", agent::error_body(&error));
            Ok(agent::exit_code(&error))
        }
        result => result,
    }
}

async fn dispatch(command: ClientCommand) -> anyhow::Result<i32> {
    match command {
        ClientCommand::Auth { command } => auth(command).await,
        ClientCommand::Run { command, target } => {
            let (api, repo) = connect(&target, true)?;
            runs::runs(&api, &repo.expect("a repository"), command, &target.output).await
        }
        ClientCommand::Workflow { command, target } => {
            let (api, repo) = connect(&target, true)?;
            workflows(&api, &repo.expect("a repository"), command).await
        }
        ClientCommand::Artifact { command, target } => {
            let (api, repo) = connect(&target, true)?;
            artifacts(&api, &repo.expect("a repository"), command).await
        }
        ClientCommand::Pr { command, target } => {
            let (api, repo) = connect(&target, true)?;
            pulls::pulls(&api, &repo.expect("a repository"), command, &target.output).await
        }
        ClientCommand::Issue { command, target } => {
            let (api, repo) = connect(&target, true)?;
            super::issues::issues(&api, &repo.expect("a repository"), command, &target.output).await
        }
        ClientCommand::Status { branch, target } => {
            let (api, repo) = connect(&target, true)?;
            super::status::status(&api, &repo.expect("a repository"), branch, &target.output).await
        }
        ClientCommand::Api { path, method, raw_fields, fields, input, target } => {
            let needs_repo = path.contains("{owner}") || path.contains("{repo}");
            let (api, repo) = connect(&target, needs_repo)?;
            raw_api(&api, repo.as_ref(), &path, method, &raw_fields, &fields, input).await
        }
    }
}

// ------------------------------------------------------------------ where and as whom

/// The URL of `remote` (`origin` by default) of the Git repository in the current directory.
fn remote_url(remote: Option<&str>) -> Option<String> {
    git(&["remote", "get-url", remote.unwrap_or("origin")])
}

/// The API of the instance `target` names, with its token, and the repository when
/// `need_repo` (an error when there is none).
fn connect(target: &Target, need_repo: bool) -> anyhow::Result<(Api, Option<Repo>)> {
    let remote = remote_url(target.remote.as_deref()).and_then(|url| parse_remote(&url));
    if let (Some(name), None) = (&target.remote, &remote) {
        bail!("no Git remote `{name}` with an rbit URL here");
    }
    let repo = match &target.repo {
        Some(repo) => Some(Repo::parse(repo)?),
        None => remote.as_ref().map(|(_, repo)| repo.clone()),
    };
    if need_repo && repo.is_none() {
        bail!("no repository: pass --repo owner/name or --remote NAME, or run inside a clone whose `origin` is on rbit");
    }
    let stored = hosts::Hosts::load()?;
    let host = match (&target.hostname, &target.repo, &remote) {
        (Some(host), ..) => hosts::normalize(host),
        (None, None, Some((host, _))) => host.clone(),
        _ => match (&stored.default, stored.hosts.len()) {
            (Some(default), _) => default.clone(),
            (None, 1) => stored.hosts.keys().next().cloned().unwrap_or_default(),
            _ => bail!("which instance? pass --hostname, or log in with `rbit auth login`"),
        },
    };
    let token = std::env::var("RBIT_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
        .or_else(|| stored.hosts.get(&host).map(|login| login.token.clone()));
    Ok((Api::new(&host, token)?, repo))
}

async fn auth(command: AuthCommand) -> anyhow::Result<i32> {
    let mut stored = hosts::Hosts::load()?;
    match command {
        AuthCommand::Login { hostname, with_token } => {
            let host = hosts::normalize(&hostname);
            let token = match std::env::var("RBIT_TOKEN").ok().filter(|token| !token.trim().is_empty()) {
                Some(token) if !with_token => token,
                _ => {
                    use std::io::IsTerminal as _;
                    if std::io::stdin().is_terminal() {
                        eprintln!("Paste a personal access token (Settings → Access tokens) and press Enter, then Ctrl-D:");
                    }
                    let mut token = String::new();
                    std::io::stdin().read_to_string(&mut token).context("cannot read the token")?;
                    token.trim().to_owned()
                }
            };
            if token.is_empty() {
                bail!("no token given");
            }
            let api = Api::new(&host, Some(token.clone()))?;
            let user = api.get("user").await?;
            let login = user["login"].as_str().ok_or_else(|| anyhow!("{host} did not say who the token belongs to"))?.to_owned();
            stored.hosts.insert(host.clone(), hosts::Login { user: login.clone(), token });
            stored.default.get_or_insert(host.clone());
            stored.save()?;
            eprintln!("Logged in to {host} as {login}");
        }
        AuthCommand::Status => {
            if stored.hosts.is_empty() {
                println!("Not logged in to any instance.");
                return Ok(1);
            }
            let mut failed = false;
            for (host, login) in &stored.hosts {
                let marker = if stored.default.as_deref() == Some(host.as_str()) { " (default)" } else { "" };
                match Api::new(host, Some(login.token.clone()))?.get("user").await {
                    Ok(user) => println!("{host}{marker}: logged in as {}", user["login"].as_str().unwrap_or(&login.user)),
                    Err(error) => {
                        failed = true;
                        println!("{host}{marker}: {error}");
                    }
                }
            }
            return Ok(i32::from(failed));
        }
        AuthCommand::Logout { hostname } => {
            let host = hosts::normalize(&hostname);
            if stored.hosts.remove(&host).is_none() {
                bail!("not logged in to {host}");
            }
            if stored.default.as_deref() == Some(host.as_str()) {
                stored.default = stored.hosts.keys().next().cloned();
            }
            stored.save()?;
            eprintln!("Logged out of {host}");
        }
        AuthCommand::Token { hostname } => {
            let Some(host) = hostname.map(|host| hosts::normalize(&host)).or(stored.default.clone()) else {
                bail!("which instance? pass --hostname");
            };
            let login = stored.hosts.get(&host).ok_or_else(|| anyhow!("not logged in to {host}"))?;
            println!("{}", login.token);
        }
    }
    Ok(0)
}

// ------------------------------------------------------------------ runs

fn actions(repo: &Repo, rest: &str) -> String {
    format!("repos/{repo}/actions/{rest}")
}

/// `run download`: the run's artifacts called `name` (every one when empty) whose name
/// matches `pattern`, each unpacked into a directory of its own under `dir`, or into `dir`
/// itself when one is named.
pub async fn download_run(
    api: &Api,
    repo: &Repo,
    id: i64,
    name: &[String],
    pattern: Option<&str>,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    let listed = api.get(&actions(repo, &format!("runs/{id}/artifacts?per_page=100"))).await?;
    let chosen: Vec<Value> = listed["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|artifact| {
            let artifact_name = text(artifact, "name");
            (name.is_empty() || name.iter().any(|wanted| wanted == artifact_name))
                && pattern.is_none_or(|pattern| wildcard(pattern, artifact_name))
        })
        .cloned()
        .collect();
    if chosen.is_empty() {
        bail!("run {id} has no such artifact");
    }
    let flat = name.len() == 1;
    for artifact in &chosen {
        let target = if flat { dir.to_path_buf() } else { dir.join(text(artifact, "name")) };
        unpack(api, repo, artifact, &target).await?;
    }
    Ok(())
}

/// Downloads `artifact` as a tar.gz and unpacks it into `target`, never outside it.
async fn unpack(api: &Api, repo: &Repo, artifact: &Value, target: &std::path::Path) -> anyhow::Result<()> {
    let bytes = api.download(&actions(repo, &format!("artifacts/{}/tar.gz", artifact["id"]))).await?;
    std::fs::create_dir_all(target).with_context(|| format!("cannot create {}", target.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    let mut files = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        // `unpack_in` refuses names that leave `target`.
        if entry.unpack_in(target)? {
            files += 1;
        }
    }
    eprintln!("Downloaded {} ({files} files) to {}", text(artifact, "name"), target.display());
    Ok(())
}

async fn workflows(api: &Api, repo: &Repo, command: WorkflowCommand) -> anyhow::Result<i32> {
    match command {
        WorkflowCommand::List => {
            let listed = api.get(&actions(repo, "workflows")).await?;
            for workflow in listed["workflows"].as_array().into_iter().flatten() {
                println!("{:<30} {:<40} {}", clip(text(workflow, "name"), 30), text(workflow, "path"), text(workflow, "state"));
            }
        }
        WorkflowCommand::Run { workflow, reference, inputs } => {
            let reference = match reference {
                Some(reference) => reference,
                None => api.get(&format!("repos/{repo}")).await?["default_branch"].as_str().unwrap_or("main").to_owned(),
            };
            let inputs: Map<String, Value> = inputs
                .iter()
                .map(|pair| split_pair(pair).map(|(key, value)| (key, Value::String(value))))
                .collect::<anyhow::Result<_>>()?;
            let file = workflow.rsplit('/').next().unwrap_or(&workflow);
            api.json(
                Method::POST,
                &actions(repo, &format!("workflows/{}/dispatches", encode(file))),
                Some(&json!({ "ref": reference, "inputs": inputs })),
            )
            .await?;
            eprintln!("Started {file} on {reference}; follow it with `rbit run watch`");
        }
    }
    Ok(0)
}

async fn artifacts(api: &Api, repo: &Repo, command: ArtifactCommand) -> anyhow::Result<i32> {
    match command {
        ArtifactCommand::List { run } => {
            let path = match run {
                Some(run) => actions(repo, &format!("runs/{run}/artifacts?per_page=100")),
                None => actions(repo, "artifacts?per_page=100"),
            };
            for artifact in api.get(&path).await?["artifacts"].as_array().into_iter().flatten() {
                println!(
                    "{:<10} {:<40} {:>12} {}",
                    artifact["id"],
                    clip(text(artifact, "name"), 40),
                    artifact["size_in_bytes"],
                    artifact["workflow_run"]["id"]
                );
            }
        }
        ArtifactCommand::Download { artifact, run, dir } => {
            let found = if let Ok(id) = artifact.parse::<i64>() {
                api.get(&actions(repo, &format!("artifacts/{id}"))).await?
            } else {
                let path = match run {
                    Some(run) => actions(repo, &format!("runs/{run}/artifacts?per_page=100")),
                    None => actions(repo, &format!("artifacts?per_page=1&name={}", encode(&artifact))),
                };
                let listed = api.get(&path).await?;
                listed["artifacts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|candidate| text(candidate, "name") == artifact)
                    .cloned()
                    .ok_or_else(|| anyhow!("no artifact `{artifact}`"))?
            };
            unpack(api, repo, &found, &dir).await?;
        }
    }
    Ok(0)
}

// ------------------------------------------------------------------ the raw API

async fn raw_api(
    api: &Api,
    repo: Option<&Repo>,
    path: &str,
    method: Option<String>,
    raw_fields: &[String],
    fields: &[String],
    input: Option<PathBuf>,
) -> anyhow::Result<i32> {
    let mut path = path.trim_start_matches('/').trim_start_matches("api/v1/").to_owned();
    if let Some(repo) = repo {
        path = path.replace("{owner}", &repo.owner).replace("{repo}", &repo.name);
    }
    let mut body = Map::new();
    for pair in raw_fields {
        let (key, value) = split_pair(pair)?;
        body.insert(key, Value::String(value));
    }
    for pair in fields {
        let (key, value) = split_pair(pair)?;
        let typed = match value.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            number if number.parse::<f64>().is_ok() => serde_json::from_str(number).unwrap_or(Value::String(value.clone())),
            _ => Value::String(value.clone()),
        };
        body.insert(key, typed);
    }
    let payload = match input {
        Some(file) if file.as_os_str() == "-" => {
            let mut text = Vec::new();
            std::io::stdin().read_to_end(&mut text)?;
            Some(text)
        }
        Some(file) => Some(std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?),
        None => None,
    };
    let method = match method {
        Some(method) => Method::from_bytes(method.to_ascii_uppercase().as_bytes()).map_err(|_| anyhow!("`{method}` is not a method"))?,
        None if !body.is_empty() || payload.is_some() => Method::POST,
        None => Method::GET,
    };
    let send = if method == Method::GET && !body.is_empty() {
        let query: Vec<String> = body.iter().map(|(key, value)| format!("{}={}", encode(key), encode(&value_text(value)))).collect();
        path = format!("{path}{}{}", if path.contains('?') { "&" } else { "?" }, query.join("&"));
        None
    } else {
        payload.or_else(|| (!body.is_empty()).then(|| serde_json::to_vec(&body).unwrap_or_default()))
    };
    let (status, kind, bytes) = api.raw(method, &path, send).await?;
    if kind.contains("json") {
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
            Err(_) => println!("{}", String::from_utf8_lossy(&bytes)),
        }
    } else {
        use std::io::Write as _;
        std::io::stdout().write_all(&bytes)?;
    }
    if !status.is_success() {
        eprintln!("rbit api: the server answered {status}");
        return Ok(1);
    }
    Ok(0)
}
