//! The client's subcommands, as close to `gh`'s as rbit's API allows.

use std::{collections::BTreeMap, io::Read as _, path::PathBuf, time::Duration};

use anyhow::{Context as _, anyhow, bail};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{Map, Value, json};

use super::{Api, Repo, hosts, parse_remote};

/// Where a command works: the repository and the instance.
#[derive(Debug, Clone, Args)]
pub struct Target {
    /// The repository, `owner/name`; the `origin` remote's when not given.
    #[arg(short = 'R', long, global = true)]
    repo: Option<String>,
    /// The instance, `https://git.example.com`; the remote's, or the one logged in to.
    #[arg(long, env = "RBIT_HOST", global = true)]
    hostname: Option<String>,
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
        #[arg(short = 'L', long, default_value_t = 20)]
        limit: u32,
        /// Print the runs as JSON.
        #[arg(long)]
        json: bool,
    },
    /// A run and its jobs; the newest run when no id is given.
    View {
        run: Option<i64>,
        /// Print every job's log.
        #[arg(long)]
        log: bool,
        /// Print the logs of the jobs that failed.
        #[arg(long)]
        log_failed: bool,
        /// Only this job.
        #[arg(short, long)]
        job: Option<i64>,
        #[arg(long)]
        json: bool,
        /// Exit with 1 when the run did not succeed.
        #[arg(long)]
        exit_status: bool,
    },
    /// Follow a run until it finishes; the newest run when no id is given.
    Watch {
        run: Option<i64>,
        /// Exit with 1 when the run did not succeed.
        #[arg(long)]
        exit_status: bool,
        /// Seconds between looks.
        #[arg(short, long, default_value_t = 3)]
        interval: u64,
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
    /// Download a run's artifacts; the newest run when no id is given.
    Download {
        run: Option<i64>,
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
        #[arg(short = 'L', long, default_value_t = 30)]
        limit: u32,
    },
    /// A pull request.
    View {
        number: u64,
        #[arg(long)]
        json: bool,
    },
    /// Merge a pull request.
    Merge {
        number: u64,
        #[arg(long, conflicts_with_all = ["squash", "rebase"])]
        merge: bool,
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        #[arg(long)]
        rebase: bool,
    },
    /// The checks of a pull request's head: exit 0 when all passed, 8 while some are pending.
    Checks {
        number: u64,
        /// Wait until every check finished.
        #[arg(long)]
        watch: bool,
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
}

/// Runs `command`; the process exit code.
pub async fn run(command: ClientCommand) -> anyhow::Result<i32> {
    match command {
        ClientCommand::Auth { command } => auth(command).await,
        ClientCommand::Run { command, target } => {
            let (api, repo) = connect(&target, true)?;
            runs(&api, &repo.expect("a repository"), command).await
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
            pulls(&api, &repo.expect("a repository"), command).await
        }
        ClientCommand::Api { path, method, raw_fields, fields, input, target } => {
            let needs_repo = path.contains("{owner}") || path.contains("{repo}");
            let (api, repo) = connect(&target, needs_repo)?;
            raw_api(&api, repo.as_ref(), &path, method, &raw_fields, &fields, input).await
        }
    }
}

// ------------------------------------------------------------------ where and as whom

/// The `origin` remote of the Git repository in the current directory.
fn origin() -> Option<String> {
    let output = std::process::Command::new("git").args(["remote", "get-url", "origin"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The API of the instance `target` names, with its token, and the repository when
/// `need_repo` (an error when there is none).
fn connect(target: &Target, need_repo: bool) -> anyhow::Result<(Api, Option<Repo>)> {
    let remote = origin().and_then(|url| parse_remote(&url));
    let repo = match &target.repo {
        Some(repo) => Some(Repo::parse(repo)?),
        None => remote.as_ref().map(|(_, repo)| repo.clone()),
    };
    if need_repo && repo.is_none() {
        bail!("no repository: pass --repo owner/name, or run inside a clone whose `origin` is on rbit");
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

/// The newest run's id, or `run`.
async fn run_or_newest(api: &Api, repo: &Repo, run: Option<i64>) -> anyhow::Result<i64> {
    if let Some(run) = run {
        return Ok(run);
    }
    let listed = api.get(&actions(repo, "runs?per_page=1")).await?;
    listed["workflow_runs"][0]["id"].as_i64().ok_or_else(|| anyhow!("{repo} has no runs"))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

/// `completed/success`, or the status while it runs.
fn state(value: &Value) -> String {
    match value["conclusion"].as_str() {
        Some(conclusion) => conclusion.to_owned(),
        None => text(value, "status").to_owned(),
    }
}

async fn runs(api: &Api, repo: &Repo, command: RunCommand) -> anyhow::Result<i32> {
    match command {
        RunCommand::List { workflow, branch, event, status, limit, json } => {
            let mut query = vec![format!("per_page={}", limit.clamp(1, 100))];
            for (key, value) in [("branch", &branch), ("event", &event), ("status", &status)] {
                if let Some(value) = value {
                    query.push(format!("{key}={}", encode(value)));
                }
            }
            let listed = api.get(&actions(repo, &format!("runs?{}", query.join("&")))).await?;
            let mut items: Vec<Value> = listed["workflow_runs"].as_array().cloned().unwrap_or_default();
            if let Some(workflow) = &workflow {
                items.retain(|run| {
                    text(run, "path").rsplit('/').next() == Some(workflow.as_str())
                        || text(run, "path") == workflow
                        || text(run, "name") == workflow
                });
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&items)?);
                return Ok(0);
            }
            for run in &items {
                println!(
                    "{:<12} {:<20} {:<40} {:<20} {:<18} {}",
                    state(run),
                    clip(text(run, "name"), 20),
                    clip(text(run, "display_title"), 40),
                    clip(text(run, "head_branch"), 20),
                    text(run, "event"),
                    run["id"]
                );
            }
        }
        RunCommand::View { run, log, log_failed, job, json, exit_status } => {
            return view_run(api, repo, run, View { log, log_failed, job, json, exit_status }).await;
        }
        RunCommand::Watch { run, exit_status, interval } => return watch_run(api, repo, run, exit_status, interval).await,
        RunCommand::Rerun { run, failed, job } => {
            let path = match (job, failed) {
                (Some(job), _) => actions(repo, &format!("jobs/{job}/rerun")),
                (None, true) => actions(repo, &format!("runs/{run}/rerun-failed-jobs")),
                (None, false) => actions(repo, &format!("runs/{run}/rerun")),
            };
            api.json(Method::POST, &path, Some(&json!({}))).await?;
            eprintln!("Run {run} runs again");
        }
        RunCommand::Cancel { run } => {
            api.json(Method::POST, &actions(repo, &format!("runs/{run}/cancel")), Some(&json!({}))).await?;
            eprintln!("Run {run} was cancelled");
        }
        RunCommand::Download { run, name, pattern, dir } => download_run(api, repo, run, &name, pattern.as_deref(), &dir).await?,
    }
    Ok(0)
}

/// What `run view` shows besides the summary.
#[expect(clippy::struct_excessive_bools, reason = "the command's flags")]
struct View {
    log: bool,
    log_failed: bool,
    job: Option<i64>,
    json: bool,
    exit_status: bool,
}

/// `run view`: the run, its jobs and the steps that did not succeed, and the logs asked for.
async fn view_run(api: &Api, repo: &Repo, run: Option<i64>, view: View) -> anyhow::Result<i32> {
    let View { log, log_failed, job, json, exit_status } = view;
    let id = run_or_newest(api, repo, run).await?;
    let run = api.get(&actions(repo, &format!("runs/{id}"))).await?;
    let jobs = api.get(&actions(repo, &format!("runs/{id}/jobs?per_page=100"))).await?;
    let jobs: Vec<Value> = jobs["jobs"].as_array().cloned().unwrap_or_default();
    if json {
        println!("{}", serde_json::to_string_pretty(&json!({ "run": run, "jobs": jobs }))?);
    } else {
        println!("{} #{} {} ({})", text(&run, "name"), run["run_number"], text(&run, "display_title"), state(&run));
        println!(
            "{} on {} at {}",
            text(&run, "event"),
            text(&run, "head_branch"),
            &text(&run, "head_sha")[..text(&run, "head_sha").len().min(12)]
        );
        println!("{}", text(&run, "html_url"));
        for job in jobs.iter().filter(|candidate| job.is_none_or(|wanted| candidate["id"].as_i64() == Some(wanted))) {
            println!("  {:<12} {} ({})", state(job), text(job, "name"), job["id"]);
            for step in job["steps"].as_array().into_iter().flatten() {
                if step["conclusion"].as_str() != Some("success") {
                    println!("      {:<10} {}", state(step), text(step, "name"));
                }
            }
        }
    }
    if log || log_failed {
        for entry in &jobs {
            let wanted = job.is_none_or(|wanted| entry["id"].as_i64() == Some(wanted));
            let failed = entry["conclusion"].as_str() == Some("failure");
            if !wanted || (log_failed && !failed) {
                continue;
            }
            let bytes = api.download(&actions(repo, &format!("jobs/{}/logs", entry["id"]))).await?;
            for line in String::from_utf8_lossy(&bytes).lines() {
                println!("{}\t{line}", text(entry, "name"));
            }
        }
    }
    if exit_status && run["conclusion"].as_str() != Some("success") {
        return Ok(1);
    }
    Ok(0)
}

/// `run watch`: each job's state as it changes, until the run completes.
async fn watch_run(api: &Api, repo: &Repo, run: Option<i64>, exit_status: bool, interval: u64) -> anyhow::Result<i32> {
    let id = run_or_newest(api, repo, run).await?;
    let mut seen: BTreeMap<i64, String> = BTreeMap::new();
    loop {
        let run = api.get(&actions(repo, &format!("runs/{id}"))).await?;
        let jobs = api.get(&actions(repo, &format!("runs/{id}/jobs?per_page=100"))).await?;
        for job in jobs["jobs"].as_array().into_iter().flatten() {
            let (key, now) = (job["id"].as_i64().unwrap_or_default(), state(job));
            if seen.get(&key) != Some(&now) {
                println!("{:<12} {}", now, text(job, "name"));
                seen.insert(key, now);
            }
        }
        if run["status"].as_str() == Some("completed") {
            println!("Run #{} {}: {}", run["run_number"], text(&run, "name"), state(&run));
            return Ok(i32::from(exit_status && run["conclusion"].as_str() != Some("success")));
        }
        tokio::time::sleep(Duration::from_secs(interval.max(1))).await;
    }
}

/// `run download`: the run's artifacts called `name` (every one when empty) whose name
/// matches `pattern`, each unpacked into a directory of its own under `dir`, or into `dir`
/// itself when one is named.
async fn download_run(
    api: &Api,
    repo: &Repo,
    run: Option<i64>,
    name: &[String],
    pattern: Option<&str>,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    let id = run_or_newest(api, repo, run).await?;
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

// ------------------------------------------------------------------ pull requests

/// The current Git branch.
fn current_branch() -> Option<String> {
    let output = std::process::Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"]).output().ok()?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && branch != "HEAD").then_some(branch)
}

async fn pulls(api: &Api, repo: &Repo, command: PrCommand) -> anyhow::Result<i32> {
    match command {
        PrCommand::Create { title, body, base, head, draft } => {
            let head = head.or_else(current_branch).ok_or_else(|| anyhow!("which branch? pass --head"))?;
            let base = match base {
                Some(base) => base,
                None => api.get(&format!("repos/{repo}")).await?["default_branch"].as_str().unwrap_or("main").to_owned(),
            };
            let created = api
                .json(
                    Method::POST,
                    &format!("repos/{repo}/pulls"),
                    Some(&json!({ "title": title, "body": body, "base": base, "head": head, "draft": draft })),
                )
                .await?;
            println!("{}", text(&created, "html_url"));
        }
        PrCommand::List { state, limit } => {
            let listed = api.get(&format!("repos/{repo}/pulls?state={}&per_page={}", encode(&state), limit.clamp(1, 100))).await?;
            for pull in listed.as_array().into_iter().flatten() {
                println!(
                    "#{:<6} {:<50} {} <- {}",
                    pull["number"],
                    clip(text(pull, "title"), 50),
                    text(&pull["base"], "ref"),
                    text(&pull["head"], "ref")
                );
            }
        }
        PrCommand::View { number, json } => {
            let pull = api.get(&format!("repos/{repo}/pulls/{number}")).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&pull)?);
            } else {
                let state = if pull["merged"].as_bool() == Some(true) { "merged" } else { text(&pull, "state") };
                println!("#{number} {} ({state})", text(&pull, "title"));
                println!(
                    "{} wants to merge {} into {}",
                    text(&pull["user"], "login"),
                    text(&pull["head"], "ref"),
                    text(&pull["base"], "ref")
                );
                println!("{}", text(&pull, "html_url"));
                if !text(&pull, "body").is_empty() {
                    println!("\n{}", text(&pull, "body"));
                }
            }
        }
        PrCommand::Merge { number, merge: _, squash, rebase } => {
            let method = if squash {
                "squash"
            } else if rebase {
                "rebase"
            } else {
                "merge"
            };
            let merged =
                api.json(Method::PUT, &format!("repos/{repo}/pulls/{number}/merge"), Some(&json!({ "merge_method": method }))).await?;
            eprintln!("Merged #{number} ({method}): {}", text(&merged, "sha"));
        }
        PrCommand::Checks { number, watch, interval } => loop {
            let pull = api.get(&format!("repos/{repo}/pulls/{number}")).await?;
            let sha = text(&pull["head"], "sha").to_owned();
            let status = api.get(&format!("repos/{repo}/commits/{sha}/status")).await?;
            let statuses = status["statuses"].as_array().cloned().unwrap_or_default();
            let pending = statuses.iter().any(|status| text(status, "state") == "pending");
            if !watch || !pending {
                for status in &statuses {
                    println!("{:<8} {:<50} {}", text(status, "state"), clip(text(status, "context"), 50), text(status, "target_url"));
                }
                if statuses.is_empty() {
                    println!("no checks reported on {}", &sha[..sha.len().min(12)]);
                }
                let failed = statuses.iter().any(|status| matches!(text(status, "state"), "failure" | "error"));
                return Ok(if failed {
                    1
                } else if pending {
                    8
                } else {
                    0
                });
            }
            tokio::time::sleep(Duration::from_secs(interval.max(1))).await;
        },
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

// ------------------------------------------------------------------ helpers

/// A field's value in a query string: text as is, anything else as JSON.
fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn split_pair(pair: &str) -> anyhow::Result<(String, String)> {
    let (key, value) = pair.split_once('=').ok_or_else(|| anyhow!("`{pair}` is not `key=value`"))?;
    Ok((key.to_owned(), value.to_owned()))
}

/// `text` for a URL's query or path segment.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width { text.to_owned() } else { format!("{}…", text.chars().take(width - 1).collect::<String>()) }
}

/// Whether `name` matches `pattern`, `*` for any run of characters and `?` for one.
fn wildcard(pattern: &str, name: &str) -> bool {
    fn go(pattern: &[char], name: &[char]) -> bool {
        match (pattern.split_first(), name.split_first()) {
            (None, None) => true,
            (Some(('*', rest)), _) => go(rest, name) || (!name.is_empty() && go(pattern, &name[1..])),
            (Some(('?', rest)), Some((_, others))) => go(rest, others),
            (Some((expected, rest)), Some((actual, others))) => expected == actual && go(rest, others),
            _ => false,
        }
    }
    go(&pattern.chars().collect::<Vec<_>>(), &name.chars().collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(split_pair("a=b=c").unwrap(), ("a".to_owned(), "b=c".to_owned()));
        assert!(split_pair("nope").is_err());
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        assert!(wildcard("dist-*", "dist-linux") && !wildcard("dist-?", "dist-xy"));
        assert_eq!(clip("abcdef", 4), "abc…");
    }
}
