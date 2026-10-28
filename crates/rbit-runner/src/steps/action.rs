//! Actions that are not built in (Actions: third-party actions).
//!
//! A step's `uses:` names one of three kinds of action ([`ActionSource`]):
//!
//! * `owner/repo[/path]@ref`: fetched with `git` from `[actions] url` (`https://github.com`
//!   by default), without the task's token or any other credential. The reference is
//!   resolved to its commit first (`git ls-remote`; a 40-hex reference is used as it is),
//!   and the action is kept at that commit under `<work_dir>/actions/<owner>/<repo>/<sha>`,
//!   so a moved tag is fetched again and a pinned commit never is. The log names the commit.
//!   Each job gets its own copy in the task's `temp` directory, which containers see at
//!   `/__w/_temp/_actions/…` and which the job may change without touching the cache.
//! * `./path`: a directory of the workspace (usually checked out by an earlier step). In a
//!   container job the workspace is the job's to change, so `action.yml` is read without
//!   following a link, and a path through a link is refused.
//! * `docker://image`: the image run as a Docker action with the step's `with.args` and
//!   `with.entrypoint`.
//!
//! `[actions] allow` decides which repository and image actions may run; the workspace's
//! own always may.
//!
//! The action's `action.yml` (or `action.yaml`) says how it runs:
//!
//! * **JavaScript** — `node <main>` with the runner's Node ([`crate::node`]), on the host or
//!   in the job container. `pre` runs right before `main` (rbit fetches actions when their
//!   step comes, not at the start of the job). `post` runs after the job's last step, in
//!   reverse order of the steps that registered one, as a step of its own (`Post Run …`),
//!   when its `post-if` holds (`always()` by default), with what `main` wrote to
//!   `GITHUB_STATE` as `STATE_<name>`.
//! * **Composite** — its steps, run as part of this step: their `inputs` context is the
//!   action's inputs, `github.action_path` its directory, `steps` their own; a failing step
//!   fails the composite unless it has `continue-on-error`. Its outputs are its `outputs`
//!   evaluated over those steps. Composites nest at most [`MAX_DEPTH`] deep.
//! * **Docker** — the image (`Dockerfile` built from the action, or `docker://…` pulled)
//!   run by the container engine with the workspace at `/github/workspace` and the env files
//!   at `/github/file_commands`, as GitHub runs it, on the job's network in a container job.
//!   It needs Docker or Podman on the runner host.
//!
//! Inputs are the step's `with:` over the action's defaults (expressions such as
//! `${{ github.token }}` are evaluated), passed as `INPUT_<NAME>`; an input the action
//! does not declare is passed too, with a warning, as on GitHub. `GITHUB_ACTION_PATH`,
//! `GITHUB_ACTION_REPOSITORY` and `GITHUB_ACTION_REF` name the action.

use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::Duration,
};

use rbit_workflow::{
    action::{ActionManifest, DockerRuns, NodeRuns, Runs, input_variable, merge_inputs, parse_action},
    expr::Condition,
    limits::Limits,
    protocol::StepStatus,
    spec::{ActionSource, ActionStep, StepAction, StepTemplate},
};
use serde_json::{Map, Value, json};
use tokio::{process::Command, sync::Mutex};

use super::{
    Outcome,
    run::{StepProcess, execute},
};
use crate::{
    backend::Backend,
    envfiles::EnvFiles,
    job::{Job, join},
    process::{self, Exit},
};

/// How deep composite actions may nest.
pub const MAX_DEPTH: usize = 9;

/// Longest a `git` command fetching an action may take.
const GIT_TIMEOUT: Duration = Duration::from_mins(5);

/// One fetch at a time, so two jobs never write the same cache directory.
static FETCHING: Mutex<()> = Mutex::const_new(());

/// An action's `post` script, run after the job's steps.
#[derive(Debug, Clone)]
pub struct PostStep {
    /// `Post Run actions/setup-node@v4`.
    pub name: String,
    /// `post-if`; `None` is `always()`.
    pub condition: Option<String>,
    pub process: StepProcess,
    pub step_env: BTreeMap<String, String>,
}

/// Where a fetched or local action is.
#[derive(Debug, Clone)]
struct Located {
    /// Where its `action.yml` and `Dockerfile` are read: the runner's own cache for a fetched
    /// action (never the job's copy, which a container job can change), the workspace for a
    /// local one (read while the job's container is paused, through no link).
    source: PathBuf,
    /// As the step sees it: the job's copy, or the workspace directory.
    view: String,
    /// `owner/repo`, or empty for a local action.
    repository: String,
    /// The reference as written, or empty.
    reference: String,
}

/// Runs `action` as step `index`. `depth` counts the composites around it.
pub fn action<'a>(
    job: &'a mut Job,
    index: usize,
    step: &'a StepTemplate,
    action: &'a ActionStep,
    step_env: &'a BTreeMap<String, String>,
    timeout: Option<Duration>,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
    Box::pin(async move {
        job.reporter.log(&format!("##[group]Run {}", action.uses));
        for (name, value) in &action.with {
            job.reporter.log(&format!("  {name}: {value}"));
        }
        job.reporter.log("##[endgroup]");
        if depth > MAX_DEPTH {
            return job.fail(&format!("composite actions nest more than {MAX_DEPTH} deep"));
        }
        if let Some(pattern) = refused(&job.actions.allow, &action.source) {
            return job.fail(&format!("`{}` is not allowed on this runner ({pattern}); see `[actions] allow`", action.uses));
        }
        // Actions: settings API.
        if let Some(rule) = job.spec.actions_policy.refuses(&action.source) {
            let why = match rule {
                rbit_workflow::spec::ActionsRule::LocalOnly => "only the repository's own actions are allowed",
                rbit_workflow::spec::ActionsRule::Selected { .. } => "it is not among the selected actions",
            };
            return job.fail(&format!("`{}` is not allowed by the repository's Actions permissions: {why}", action.uses));
        }
        let located = match &action.source {
            ActionSource::Docker { image } => return docker_step(job, index, action, image, step_env, timeout).await,
            ActionSource::Local { path } => local(job, path),
            ActionSource::Repository { owner, repo, path, reference } => repository(job, owner, repo, path, reference).await,
        };
        let located = match located {
            Ok(located) => located,
            Err(error) => return job.fail(&error),
        };
        // A local action is in the workspace, which a container job can change: it is read
        // while the container is paused.
        let local = matches!(action.source, ActionSource::Local { .. });
        if local && let Err(error) = job.backend.pause(&job.reporter).await {
            return job.fail(&format!("cannot pause the job container to read `{}`: {error}", action.uses));
        }
        let manifest = read_manifest(&located.source);
        if local && let Err(error) = job.backend.resume().await {
            return job.fail(&format!("cannot resume the job container after reading `{}`: {error}", action.uses));
        }
        let manifest = match manifest {
            Ok(manifest) => manifest,
            Err(error) => return job.fail(&format!("`{}`: {error}", action.uses)),
        };
        let inputs = match evaluate_inputs(job, action, &manifest) {
            Ok(inputs) => inputs,
            Err(error) => return job.fail(&error),
        };
        let mut extra: Vec<(String, String)> = inputs.iter().map(|(name, value)| (input_variable(name), value.clone())).collect();
        extra.extend([
            ("GITHUB_ACTION_PATH".to_owned(), located.view.clone()),
            ("GITHUB_ACTION_REPOSITORY".to_owned(), located.repository.clone()),
            ("GITHUB_ACTION_REF".to_owned(), located.reference.clone()),
            ("GITHUB_ACTION".to_owned(), step.id.clone().unwrap_or_else(|| format!("__{}", located.repository.replace('/', "_")))),
        ]);
        let step = Running { index, uses: &action.uses, step_env, timeout };
        match manifest.runs.clone() {
            Runs::Node(node) => javascript(job, &step, &node, &located, extra).await,
            Runs::Composite(steps) => composite(job, &step, &manifest, &steps, &inputs, &located, &extra, depth).await,
            Runs::Docker(docker) => container_action(job, &step, &docker, &inputs, &located, extra).await,
        }
    })
}

/// The step an action runs as.
struct Running<'a> {
    index: usize,
    /// `uses:`, for the post step's name.
    uses: &'a str,
    step_env: &'a BTreeMap<String, String>,
    timeout: Option<Duration>,
}

/// A JavaScript action: `pre`, `main`, and its `post` registered for after the job.
async fn javascript(job: &mut Job, step: &Running<'_>, node: &NodeRuns, located: &Located, extra: Vec<(String, String)>) -> Outcome {
    let program = match node_program(job).await {
        Ok(program) => program,
        Err(error) => return job.fail(&error),
    };
    let windows = job.backend.windows();
    let script = |file: &str| join(&located.view, &file.replace('/', if windows { "\\" } else { "/" }), windows);
    let cwd = job.backend.view().workspace.clone();
    let process = |file: &str, extra: Vec<(String, String)>| StepProcess {
        program: program.clone(),
        args: vec![script(file)],
        raw_args: None,
        cwd: cwd.clone(),
        extra,
    };
    // What `pre` and `main` write to GITHUB_STATE reaches the scripts after them, as STATE_*.
    let mut state = BTreeMap::new();
    let with_state = |state: &BTreeMap<String, String>| {
        let mut extra = extra.clone();
        extra.extend(state.iter().map(|(name, value)| (format!("STATE_{name}"), value.clone())));
        extra
    };
    let mut outcome = None;
    if let Some(pre) = &node.pre {
        match condition(job, node.pre_if.as_deref()) {
            Ok(true) => {
                let (ran, written) = execute(job, step.index, process(pre, with_state(&state)), step.step_env, step.timeout).await;
                state.extend(written);
                if ran.status != StepStatus::Success {
                    outcome = Some(ran);
                }
            }
            Ok(false) => {}
            Err(failed) => outcome = Some(failed),
        }
    }
    let outcome = if let Some(failed) = outcome {
        failed
    } else {
        let (ran, written) = execute(job, step.index, process(&node.main, with_state(&state)), step.step_env, step.timeout).await;
        state.extend(written);
        ran
    };
    // `post` runs after the job's steps whatever `pre` and `main` did, when `post-if` holds.
    if let Some(post) = &node.post {
        let room = 2 * rbit_workflow::spec::MAX_STEPS - job.spec.template.steps.len();
        if job.post_steps.len() < room {
            job.post_steps.push(PostStep {
                name: format!("Post Run {}", step.uses),
                condition: node.post_if.clone(),
                process: process(post, with_state(&state)),
                step_env: step.step_env.clone(),
            });
        } else {
            job.reporter.log(&format!("##[warning]the job has {room} post steps already; `{}`'s does not run", step.uses));
        }
    }
    outcome
}

/// A Docker action of `action.yml`: its `args` and `env` evaluated with the `inputs` context.
async fn container_action(
    job: &mut Job,
    step: &Running<'_>,
    docker: &DockerRuns,
    inputs: &[(String, String)],
    located: &Located,
    extra: Vec<(String, String)>,
) -> Outcome {
    let image = match docker.image.strip_prefix("docker://") {
        Some(image) if valid_image(image) => DockerImage::Pull(image.to_owned()),
        Some(image) => return job.fail(&format!("`runs.image`: `{}` is not an image reference", image.escape_debug())),
        None => match dockerfile(&located.source, &docker.image) {
            Ok(path) => DockerImage::Build(path, located.source.clone()),
            Err(error) => return job.fail(&format!("`runs.image`: {error}")),
        },
    };
    // `args` and `env` are the action's: they may use `inputs`, never `secrets`.
    let saved = job.contexts.get("inputs").cloned();
    let context: Map<String, Value> = inputs.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
    job.contexts.insert("inputs", Value::Object(context));
    let (args, env) = without_secrets(job, |job| {
        let args = docker.args.iter().map(|arg| job.render(arg)).collect::<Result<Vec<_>, _>>();
        let env =
            docker.env.iter().map(|(name, value)| job.render(value).map(|value| (name.clone(), value))).collect::<Result<Vec<_>, _>>();
        (args, env)
    });
    match saved {
        Some(saved) => job.contexts.insert("inputs", saved),
        None => {
            job.contexts.remove("inputs");
        }
    }
    let (args, env) = match (args, env) {
        (Ok(args), Ok(env)) => (args, env),
        (Err(error), _) => return job.fail(&format!("`runs.args`: {error}")),
        (_, Err(error)) => return job.fail(&format!("`runs.env`: {error}")),
    };
    let run = DockerRun { image, entrypoint: docker.entrypoint.clone(), args, extra: [extra, env].concat() };
    run_docker(job, step.index, &run, step.step_env, step.timeout).await
}

/// Whether `image` is an image reference the engine reads as one: `name[:tag][@digest]`
/// characters only, and no leading `-`, which `docker run` would take for an option.
pub fn valid_image(image: &str) -> bool {
    !image.is_empty()
        && image.len() <= 512
        && !image.starts_with(['-', '.', '/', ':', '@'])
        && image.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b':' | b'@'))
}

/// An action's `Dockerfile`: a regular file inside `dir`, by plain names and through no link.
fn dockerfile(dir: &Path, image: &str) -> Result<PathBuf, String> {
    let relative = Path::new(image);
    if relative.is_absolute() || !relative.components().all(|component| matches!(component, std::path::Component::Normal(_))) {
        return Err(format!("`{image}` must be a file of the action"));
    }
    if crate::untrusted::crosses_link(dir, relative) {
        return Err(format!("`{image}` goes through a symbolic link"));
    }
    let path = dir.join(relative);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(format!("`{image}` is not a regular file")),
        Err(error) => Err(format!("cannot read `{image}`: {error}")),
    }
}

/// Runs `f` with the `secrets` context hidden: what an action's own `action.yml` evaluates
/// (input defaults, composite steps, Docker `args` and `env`) never sees the workflow's
/// secrets, as on GitHub. The workflow passes one with `with:` or `env:` if it means to.
fn without_secrets<T>(job: &mut Job, f: impl FnOnce(&mut Job) -> T) -> T {
    let secrets = job.contexts.remove("secrets");
    let result = f(job);
    if let Some(secrets) = secrets {
        job.contexts.insert("secrets", secrets);
    }
    result
}

/// Whether a `pre-if` / `post-if` holds now; `None` is `always()`.
fn condition(job: &Job, text: Option<&str>) -> Result<bool, Outcome> {
    let text = text.unwrap_or("always()");
    Condition::parse(text)
        .and_then(|condition| condition.evaluate(&job.eval(Some(job.phase.status()))))
        .map_err(|error| job.fail(&format!("`{text}`: {error}")))
}

/// The pattern of `[actions] allow` that refuses `source`, or `None` when it may run.
fn refused(allow: &[String], source: &ActionSource) -> Option<&'static str> {
    let name = match source {
        ActionSource::Local { .. } => return None,
        ActionSource::Repository { owner, repo, .. } => format!("{owner}/{repo}"),
        ActionSource::Docker { image } => format!("docker://{image}"),
    };
    let allowed = allow.iter().any(|pattern| super::artifact::matches(&pattern.to_ascii_lowercase(), &name.to_ascii_lowercase()));
    (!allowed).then_some("no `allow` pattern matches it")
}

/// A local action: `path` inside the workspace, reached through no link.
fn local(job: &Job, path: &str) -> Result<Located, String> {
    let host = super::checkout::target(&job.dirs.workspace, path)
        .ok_or_else(|| format!("`./{path}` must stay inside the workspace and not go through a symbolic link"))?;
    let windows = job.backend.windows();
    let view = if path.is_empty() {
        job.backend.view().workspace.clone()
    } else {
        join(&job.backend.view().workspace, &path.replace('/', if windows { "\\" } else { "/" }), windows)
    };
    Ok(Located { source: host, view, repository: String::new(), reference: String::new() })
}

/// A repository's action: resolved, fetched into the cache unless it is there, and copied
/// into the task.
async fn repository(job: &Job, owner: &str, repo: &str, path: &str, reference: &str) -> Result<Located, String> {
    let url = format!("{}/{owner}/{repo}", job.actions.url.trim_end_matches('/'));
    let git = process::find_program("git").ok_or("fetching actions needs `git` on the runner host")?;
    let env = git_env(&url, job.private_dir().ok().as_deref());
    let sha = if is_commit(reference) { reference.to_ascii_lowercase() } else { resolve(&git, &env, &url, reference).await? };
    let actions = job.work_dir.join("actions");
    let cache = actions.join(owner.to_ascii_lowercase()).join(repo.to_ascii_lowercase()).join(&sha);
    if complete(&cache).is_file() {
        job.reporter.log(&format!("Using action repository '{owner}/{repo}@{reference}' (SHA:{sha})"));
        // Kept in use: the oldest are pruned first.
        let _ = std::fs::write(complete(&cache), &sha);
    } else {
        let _fetching = FETCHING.lock().await;
        if !complete(&cache).is_file() {
            job.reporter.log(&format!("Download action repository '{owner}/{repo}@{reference}' (SHA:{sha})"));
            fetch(&git, &env, &url, &sha, &cache).await?;
            prune(&actions, MAX_CACHED_ACTIONS);
        }
    }
    // The action's own sub-path must not leave it through a link it carries.
    if !path.is_empty() && crate::untrusted::crosses_link(&cache, Path::new(path)) {
        return Err(format!("`{owner}/{repo}/{path}` goes through a symbolic link"));
    }
    // The job gets a copy in a fresh directory of its `temp`, made while its container is
    // paused: a container job can change `temp`, so it can neither have planted links on the
    // way nor race the copy.
    job.backend.pause(&job.reporter).await.map_err(|error| format!("cannot pause the job container to copy the action: {error}"))?;
    let copied = {
        let (from, temp) = (cache.clone(), job.dirs.temp.clone());
        tokio::task::spawn_blocking(move || copy_into_task(&from, &temp)).await.map_err(|error| error.to_string())
    };
    job.backend.resume().await.map_err(|error| format!("cannot resume the job container after copying the action: {error}"))?;
    let name = copied?.map_err(|error| format!("cannot copy the action into the task: {error}"))?;
    let windows = job.backend.windows();
    let separator = if windows { "\\" } else { "/" };
    let mut view = join(&job.backend.view().temp, &format!("_actions{separator}{name}"), windows);
    let mut source = cache;
    if !path.is_empty() {
        view = join(&view, &path.replace('/', separator), windows);
        source = source.join(path);
    }
    Ok(Located { source, view, repository: format!("{owner}/{repo}"), reference: reference.to_owned() })
}

/// How many fetched actions the runner keeps; the least recently used go first.
const MAX_CACHED_ACTIONS: usize = 200;

/// The file beside `cache` that marks it complete (beside, not in it: a fetched tree could
/// carry a link where the mark would be written).
fn complete(cache: &Path) -> PathBuf {
    cache.with_extension("complete")
}

fn is_commit(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `git` with no prompt, no credential (not even a helper's), only the action source's own
/// scheme, and no hook or `fsmonitor` command.
fn git_env(url: &str, hooks: Option<&Path>) -> BTreeMap<String, String> {
    let scheme = url.split_once("://").map_or("https", |(scheme, _)| scheme);
    let mut config = vec![
        ("credential.helper".to_owned(), String::new()),
        ("protocol.allow".to_owned(), "never".to_owned()),
        (format!("protocol.{scheme}.allow"), "always".to_owned()),
        ("core.fsmonitor".to_owned(), "false".to_owned()),
        ("advice.detachedHead".to_owned(), "false".to_owned()),
    ];
    if let Some(hooks) = hooks {
        config.push(("core.hooksPath".to_owned(), hooks.display().to_string()));
    }
    let mut env = BTreeMap::from([
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
        ("GCM_INTERACTIVE".to_owned(), "never".to_owned()),
        ("GIT_CONFIG_NOSYSTEM".to_owned(), "1".to_owned()),
        ("GIT_CONFIG_COUNT".to_owned(), config.len().to_string()),
    ]);
    for (index, (key, value)) in config.into_iter().enumerate() {
        env.insert(format!("GIT_CONFIG_KEY_{index}"), key);
        env.insert(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    env
}

async fn git(git: &Path, env: &BTreeMap<String, String>, dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(git);
    command.args(args).envs(env);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    process::isolate(&mut command);
    let output = process::capture(command, None, GIT_TIMEOUT).await.map_err(|error| format!("cannot run git: {error}"))?;
    if output.success {
        Ok(output.stdout)
    } else {
        Err(format!("`git {}` failed: {}", args.first().copied().unwrap_or_default(), output.stderr.trim()))
    }
}

/// The commit `reference` names at `url`: a tag (peeled) before a branch.
async fn resolve(program: &Path, env: &BTreeMap<String, String>, url: &str, reference: &str) -> Result<String, String> {
    let tag = format!("refs/tags/{reference}");
    let peeled = format!("{tag}^{{}}");
    let branch = format!("refs/heads/{reference}");
    let listed = git(program, env, None, &["ls-remote", "--", url, &tag, &peeled, &branch]).await?;
    let refs: BTreeMap<&str, &str> =
        listed.lines().filter_map(|line| line.split_once('\t')).map(|(sha, name)| (name.trim(), sha.trim())).collect();
    [peeled.as_str(), tag.as_str(), branch.as_str()]
        .iter()
        .find_map(|name| refs.get(name).filter(|sha| is_commit(sha)).map(|sha| sha.to_ascii_lowercase()))
        .ok_or_else(|| format!("`{reference}` is not a tag or branch of {url}"))
}

/// Fetches commit `sha` of `url` into `cache` (through a temporary directory, so a
/// half-done fetch is never used), without its `.git`.
async fn fetch(program: &Path, env: &BTreeMap<String, String>, url: &str, sha: &str, cache: &Path) -> Result<(), String> {
    let parent = cache.parent().ok_or("the action cache has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let staging = parent.join(format!(".fetch-{}", crate::client::idempotency_key()));
    std::fs::create_dir_all(&staging).map_err(|error| format!("cannot create {}: {error}", staging.display()))?;
    let result = async {
        git(program, env, Some(&staging), &["init", "--quiet"]).await?;
        git(program, env, Some(&staging), &["fetch", "--quiet", "--depth=1", "--no-tags", "--", url, sha]).await?;
        git(program, env, Some(&staging), &["checkout", "--quiet", "--detach", "FETCH_HEAD"]).await?;
        let head = git(program, env, Some(&staging), &["rev-parse", "HEAD"]).await?;
        if !head.trim().eq_ignore_ascii_case(sha) {
            return Err(format!("{url} gave commit {} for {sha}", head.trim()));
        }
        std::fs::remove_dir_all(staging.join(".git")).map_err(|error| format!("cannot remove the action's .git: {error}"))?;
        if cache.exists() {
            std::fs::remove_dir_all(cache).map_err(|error| format!("cannot replace {}: {error}", cache.display()))?;
        }
        std::fs::rename(&staging, cache).map_err(|error| format!("cannot move the action into {}: {error}", cache.display()))?;
        std::fs::write(complete(cache), sha).map_err(|error| format!("cannot mark the action fetched: {error}"))
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

/// Removes the least recently used fetched actions beyond `keep`, never one used within the
/// last hour (a job may be copying it).
fn prune(actions: &Path, keep: usize) {
    let mut marks: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for owner in std::fs::read_dir(actions).into_iter().flatten().flatten() {
        for repo in std::fs::read_dir(owner.path()).into_iter().flatten().flatten() {
            for entry in std::fs::read_dir(repo.path()).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|extension| extension == "complete")
                    && let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified())
                {
                    marks.push((modified, path));
                }
            }
        }
    }
    if marks.len() <= keep {
        return;
    }
    marks.sort();
    let recent = std::time::SystemTime::now() - Duration::from_hours(1);
    let excess = marks.len() - keep;
    for (modified, mark) in marks.into_iter().take(excess) {
        if modified > recent {
            break;
        }
        let _ = std::fs::remove_file(&mark);
        let _ = std::fs::remove_dir_all(mark.with_extension(""));
    }
}

/// Copies the fetched action `from` into a fresh directory of `temp/_actions` and returns
/// its name. Run while the job's container is paused: `_actions` is replaced if it is not a
/// plain directory, the new directory is created anew, and only regular files and
/// directories are copied, each created without following a link.
fn copy_into_task(from: &Path, temp: &Path) -> std::io::Result<String> {
    fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let target = to.join(entry.file_name());
            if kind.is_dir() {
                copy(&entry.path(), &target)?;
            } else if kind.is_file() {
                let mut source = std::fs::File::open(entry.path())?;
                let mut file = crate::untrusted::create(&target, false)?;
                std::io::copy(&mut source, &mut file)?;
                // Scripts an action runs directly stay executable.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    let mode = entry.metadata()?.permissions().mode() & 0o755;
                    file.set_permissions(std::fs::Permissions::from_mode(mode))?;
                }
            }
        }
        Ok(())
    }
    let actions = temp.join("_actions");
    match std::fs::symlink_metadata(&actions) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            std::fs::remove_file(&actions)?;
            std::fs::create_dir(&actions)?;
        }
        Ok(_) => return Err(std::io::Error::other("`_actions` in the task's temp directory is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&actions)?,
        Err(error) => return Err(error),
    }
    let name = crate::client::idempotency_key();
    copy(from, &actions.join(&name))?;
    Ok(name)
}

/// `action.yml` (or `action.yaml`) in `dir`, opened without following a link: a fetched
/// action may carry links, and a local one is in a workspace a container job can change.
fn read_manifest(dir: &Path) -> Result<ActionManifest, String> {
    let mut last = None;
    for name in ["action.yml", "action.yaml"] {
        let path = dir.join(name);
        match crate::untrusted::open(&path) {
            Ok(file) => {
                use std::io::Read as _;
                let limits = Limits::default();
                let mut bytes = Vec::new();
                file.take(limits.max_bytes as u64 + 1).read_to_end(&mut bytes).map_err(|error| format!("cannot read {name}: {error}"))?;
                return parse_action(&bytes, &limits).map_err(|error| format!("{name}: {error}"));
            }
            // A refused file (a link, not a regular file) says so, whichever name it has.
            Err(error) if error.kind() != std::io::ErrorKind::NotFound && last.is_none() => last = Some(error),
            Err(_) => {}
        }
    }
    Err(match last {
        Some(error) if error.kind() != std::io::ErrorKind::NotFound => format!("cannot read action.yml: {error}"),
        _ => format!("there is no action.yml or action.yaml in {}", dir.display()),
    })
}

/// The step's inputs: `with:` over the action's defaults, evaluated, by the action's
/// spelling of each name. Inputs it does not declare are warned about and passed on.
fn evaluate_inputs(job: &mut Job, action: &ActionStep, manifest: &ActionManifest) -> Result<Vec<(String, String)>, String> {
    let unexpected: Vec<&str> = action.with.keys().filter(|key| manifest.input(key).is_none()).map(String::as_str).collect();
    if !unexpected.is_empty() {
        let valid: Vec<&str> = manifest.inputs.iter().map(|input| input.name.as_str()).collect();
        job.reporter.log(&format!(
            "##[warning]Unexpected input(s) '{}', valid inputs are ['{}']",
            unexpected.join("', '"),
            valid.join("', '")
        ));
    }
    let mut values = Vec::new();
    for (name, given) in merge_inputs(manifest, &action.with) {
        let declared = manifest.input(&name);
        if let (Some(input), Some(_)) = (declared, &given)
            && let Some(message) = &input.deprecation
        {
            job.reporter.log(&format!("##[warning]Input '{name}' has been deprecated with message: {message}"));
        }
        // The workflow's `with:` may use its secrets; the action's own defaults may not.
        let value = if let Some(template) = given {
            job.render(&template)
        } else {
            let default = declared.and_then(|input| input.default.clone()).unwrap_or_default();
            without_secrets(job, |job| job.render(&default))
        };
        let value = value.map_err(|error| format!("`with.{name}`: {error}"))?;
        values.push((name, value));
    }
    Ok(values)
}

/// The `node` program, on the host or as the job container sees it.
async fn node_program(job: &Job) -> Result<String, String> {
    let externals = job.work_dir.join("externals");
    let reporter = std::sync::Arc::clone(&job.reporter);
    let log = move |line: &str| reporter.log(line);
    match &job.backend {
        Backend::Host(_) => Ok(crate::node::host_node(job.actions.node.as_deref(), &externals, log).await?.display().to_string()),
        Backend::Container(_) => crate::node::container_node(&externals, log).await,
    }
}

/// Runs a composite action's steps as part of its step. They see the action's `inputs`,
/// `github.action_*` and their own `steps`, get `GITHUB_ACTION_PATH` and the other
/// variables `extra` holds, never the workflow's `secrets`, and not the job's
/// `defaults.run`, as on GitHub.
#[allow(clippy::too_many_arguments)] // the parts of one composite run
async fn composite(
    job: &mut Job,
    step: &Running<'_>,
    manifest: &ActionManifest,
    steps: &[StepTemplate],
    inputs: &[(String, String)],
    located: &Located,
    extra: &[(String, String)],
    depth: usize,
) -> Outcome {
    let index = step.index;
    let mut step_env = step.step_env.clone();
    step_env.extend(extra.iter().filter(|(name, _)| !name.starts_with("INPUT_")).cloned());
    let step_env = &step_env;
    let saved = Saved::enter(job, inputs, located);

    let mut failed = false;
    let mut cancelled = false;
    for sub in steps {
        let status = if cancelled {
            rbit_workflow::expr::Status::Cancelled
        } else if failed {
            rbit_workflow::expr::Status::Failure
        } else {
            job.phase.status()
        };
        let run = Condition::parse(sub.condition.as_deref().unwrap_or_default())
            .and_then(|condition| condition.evaluate(&job.eval(Some(status))));
        let run = match run {
            Ok(run) => run,
            Err(error) => {
                job.reporter.log(&format!("##[error]`if`: {error}"));
                failed = true;
                continue;
            }
        };
        if !run {
            record(job, sub, &Outcome { status: StepStatus::Skipped, outputs: BTreeMap::new() }, StepStatus::Skipped);
            continue;
        }
        let mut sub_env = step_env.clone();
        match job.render_env(&sub.env) {
            Ok(env) => sub_env.extend(env),
            Err(error) => {
                job.reporter.log(&format!("##[error]`env`: {error}"));
                failed = true;
                continue;
            }
        }
        let timeout = match job.step_timeout(sub) {
            Ok(timeout) => timeout,
            Err(error) => {
                job.reporter.log(&format!("##[error]`timeout-minutes`: {error}"));
                failed = true;
                continue;
            }
        };
        job.set_env_context(&sub_env);
        let outcome = match &sub.action {
            StepAction::Run(script) => super::run::run(job, index, sub, script, &sub_env, timeout).await,
            StepAction::Uses(builtin) => super::builtin(job, builtin, timeout).await,
            StepAction::Action(inner) => action(job, index, sub, inner, &sub_env, timeout, depth + 1).await,
        };
        job.set_env_context(step_env);
        let conclusion = match outcome.status {
            StepStatus::Failure if job.continue_on_error(sub) => StepStatus::Success,
            status => status,
        };
        match conclusion {
            StepStatus::Failure => failed = true,
            StepStatus::Cancelled => cancelled = true,
            _ => {}
        }
        record(job, sub, &outcome, conclusion);
    }

    // The outputs are the composite's, over its own steps.
    let mut outputs = BTreeMap::new();
    for output in &manifest.outputs {
        if let Some(value) = &output.value {
            match job.render(value) {
                Ok(value) => {
                    outputs.insert(output.name.clone(), value);
                }
                Err(error) => job.reporter.log(&format!("##[warning]output `{}`: {error}", output.name)),
            }
        }
    }
    saved.restore(job);
    let status = if cancelled {
        StepStatus::Cancelled
    } else if failed {
        StepStatus::Failure
    } else {
        StepStatus::Success
    };
    Outcome { status, outputs }
}

/// The job's contexts while a composite's steps run in their own.
struct Saved {
    steps: Map<String, Value>,
    inputs: Option<Value>,
    github: Option<Value>,
    secrets: Option<Value>,
    defaults: rbit_workflow::spec::RunDefaults,
}

impl Saved {
    /// Gives the composite its own `steps`, its `inputs`, and `github.action_*`, and takes
    /// away `secrets` and the job's `defaults.run`.
    fn enter(job: &mut Job, inputs: &[(String, String)], located: &Located) -> Self {
        let saved = Self {
            steps: std::mem::take(&mut job.steps),
            inputs: job.contexts.get("inputs").cloned(),
            github: job.contexts.get("github").cloned(),
            secrets: job.contexts.remove("secrets"),
            defaults: std::mem::take(&mut job.spec.template.defaults),
        };
        let mut github = saved.github.clone().unwrap_or_else(|| json!({}));
        if let Some(github) = github.as_object_mut() {
            github.insert("action_path".into(), Value::String(located.view.clone()));
            github.insert("action_repository".into(), Value::String(located.repository.clone()));
            github.insert("action_ref".into(), Value::String(located.reference.clone()));
        }
        let inputs: Map<String, Value> = inputs.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
        job.contexts.insert("inputs", Value::Object(inputs));
        job.contexts.insert("github", github);
        job.sync_contexts();
        saved
    }

    fn restore(self, job: &mut Job) {
        job.steps = self.steps;
        match self.inputs {
            Some(inputs) => job.contexts.insert("inputs", inputs),
            None => {
                job.contexts.remove("inputs");
            }
        }
        if let Some(github) = self.github {
            job.contexts.insert("github", github);
        }
        if let Some(secrets) = self.secrets {
            job.contexts.insert("secrets", secrets);
        }
        job.spec.template.defaults = self.defaults;
        job.sync_contexts();
    }
}

/// Records a composite's step in its own `steps` context.
fn record(job: &mut Job, step: &StepTemplate, outcome: &Outcome, conclusion: StepStatus) {
    if let Some(id) = &step.id {
        job.steps.insert(
            id.clone(),
            json!({
                "outputs": outcome.outputs,
                "outcome": crate::job::status_name(outcome.status),
                "conclusion": crate::job::status_name(conclusion),
            }),
        );
    }
    job.sync_contexts();
}

/// A Docker action's image.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerImage {
    /// `docker://…`: pulled.
    Pull(String),
    /// A `Dockerfile` of the action, and the action's directory as the build's context.
    Build(PathBuf, PathBuf),
}

#[derive(Debug, Clone)]
struct DockerRun {
    image: DockerImage,
    entrypoint: Option<String>,
    args: Vec<String>,
    extra: Vec<(String, String)>,
}

/// `uses: docker://image`: the image run with `with.args` (split as a shell would, quotes
/// kept together) and `with.entrypoint`; the other `with:` keys become `INPUT_*`.
async fn docker_step(
    job: &mut Job,
    index: usize,
    action: &ActionStep,
    image: &str,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
) -> Outcome {
    let render = |key: &str| action.with.get(key).map(|value| job.render(value)).transpose();
    let (args, entrypoint) = match (render("args"), render("entrypoint")) {
        (Ok(args), Ok(entrypoint)) => (args, entrypoint),
        (Err(error), _) | (_, Err(error)) => return job.fail(&error),
    };
    let args = args.map(|args| split_args(&args)).unwrap_or_default();
    let mut extra = Vec::new();
    for (key, value) in action.with.iter().filter(|(key, _)| !matches!(key.as_str(), "args" | "entrypoint")) {
        match job.render(value) {
            Ok(value) => extra.push((input_variable(key), value)),
            Err(error) => return job.fail(&format!("`with.{key}`: {error}")),
        }
    }
    let run = DockerRun { image: DockerImage::Pull(image.to_owned()), entrypoint, args, extra };
    run_docker(job, index, &run, step_env, timeout).await
}

/// `args` split as a shell would: on whitespace, with `"…"` and `'…'` kept together.
fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// Where a Docker action sees the task's directories.
pub(super) const DOCKER_WORKSPACE: &str = "/github/workspace";
const DOCKER_FILES: &str = "/github/file_commands";
pub(super) const DOCKER_TEMP: &str = "/github/runner_temp";

/// Builds or pulls the image, then runs it with the task's directories mounted, as step
/// `index`.
async fn run_docker(
    job: &mut Job,
    index: usize,
    run: &DockerRun,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
) -> Outcome {
    let (program, network) = match &job.backend {
        Backend::Container(container) => {
            let (program, network) = container.engine();
            (program.to_path_buf(), Some(network.to_owned()))
        }
        Backend::Host(_) => match process::find_program(&job.container.program) {
            Some(program) => (program, None),
            None => return job.fail(&format!("Docker actions need `{}` on the runner host", job.container.program)),
        },
    };
    let (image, built) = match &run.image {
        DockerImage::Pull(image) => (image.clone(), false),
        DockerImage::Build(dockerfile, context) => {
            // A local action's Dockerfile is in the workspace, which a container job can
            // change: it is built while the job's container is paused.
            let local = context.starts_with(&job.dirs.workspace);
            if local && let Err(error) = job.backend.pause(&job.reporter).await {
                return job.fail(&format!("cannot pause the job container to build the action: {error}"));
            }
            let built = build_image(job, &program, dockerfile, context, timeout).await;
            if local && let Err(error) = job.backend.resume().await {
                return job.fail(&format!("cannot resume the job container after building the action: {error}"));
            }
            match built {
                Ok(tag) => (tag, true),
                Err(outcome) => return outcome,
            }
        }
    };
    let outcome = run_image(job, index, run, &program, network, &image, step_env, timeout).await;
    // The action's container wrote the task's directories as its own user, root mostly.
    match &job.backend {
        Backend::Container(container) => container.wrote(),
        Backend::Host(_) => {
            if let Some(owner) = crate::backend::container::file_owner(&program).await {
                if let Err(error) = crate::backend::container::ensure_image(&program, &job.container.helper_image, &job.reporter).await {
                    return job.fail(&error);
                }
                let args = crate::backend::container::reclaim_run(
                    job.spec.task_id,
                    &job.runner_name,
                    &job.container.helper_image,
                    &job.dirs,
                    &owner,
                );
                if let Err(error) = crate::backend::container::reclaim(process::command(&program), &args).await {
                    job.reporter.log(&format!("##[warning]cannot hand the task's files back to the runner's user: {error}"));
                }
            }
        }
    }
    if built {
        // Each build is the job's own; leaving it would fill the engine's storage.
        let mut command = process::command(&program);
        command.args(["rmi", "--force", &image]);
        let _ = process::capture(command, None, Duration::from_mins(1)).await;
    }
    outcome
}

/// Builds the action's `Dockerfile` into an image of its own; its tag.
async fn build_image(job: &Job, program: &Path, dockerfile: &Path, context: &Path, timeout: Option<Duration>) -> Result<String, Outcome> {
    let tag = format!("rbit-action-{}", crate::client::idempotency_key().to_ascii_lowercase());
    job.reporter.log(&format!("Building the action's image {tag}"));
    let mut help = process::command(program);
    help.args(["build", "--help"]);
    let help = process::capture(help, None, Duration::from_mins(1)).await.map(|output| output.stdout).unwrap_or_default();
    let (limits, missing) = crate::backend::container::build_limits(&help, &job.container);
    if !missing.is_empty() {
        job.reporter.log(&format!(
            "The engine's builder (BuildKit) takes no {} limit for a build: it runs under the engine's own, within the step's timeout",
            missing.join(" or ")
        ));
    }
    let mut command = process::command(program);
    command.arg("build").arg("--tag").arg(&tag).arg("--file").arg(dockerfile).args(&limits).arg(context);
    // Buildx runs `git` on the host in a context inside a repository (the workspace, for a
    // local action) to describe an image that is removed after the step: not at all, and
    // with no hook or `fsmonitor` command of the repository's should a version still do.
    command.envs(git_env(&job.actions.url, job.private_dir().ok().as_deref()));
    command.env("BUILDX_GIT_INFO", "0").env("BUILDX_GIT_LABELS", "0");
    let signal = job.stop_signal(timeout);
    let reporter = &job.reporter;
    match process::run_streaming(command, process::MAX_LINE_BYTES, signal.wait(), &crate::process::Stopper::Host, |line| {
        reporter.log(&line);
    })
    .await
    {
        Ok(Exit::Code(Some(0))) => Ok(tag),
        Ok(Exit::Stopped) => Err(job.stopped(signal.reason())),
        Ok(_) => Err(job.fail("the action's image could not be built")),
        Err(error) => Err(job.fail(&format!("cannot run {}: {error}", program.display()))),
    }
}

/// Runs `image` as step `index`, with the task's directories mounted as GitHub's runner
/// mounts them.
#[allow(clippy::too_many_arguments)] // the parts of one `docker run`
async fn run_image(
    job: &mut Job,
    index: usize,
    run: &DockerRun,
    program: &Path,
    network: Option<String>,
    image: &str,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
) -> Outcome {
    if !crate::backend::container::valid_image(image) {
        return job.fail(&format!("`{image}` is not an image reference"));
    }
    // The step's view of the task, as GitHub's runner mounts it.
    let files = match EnvFiles::create(&job.dirs.files, index) {
        Ok(files) => files,
        Err(error) => return job.fail(&format!("cannot create the env files: {error}")),
    };
    let mut extra = files.variables(DOCKER_FILES);
    extra.extend(run.extra.iter().cloned());
    extra.extend([
        ("GITHUB_WORKSPACE".to_owned(), DOCKER_WORKSPACE.to_owned()),
        ("RBIT_WORKSPACE".to_owned(), DOCKER_WORKSPACE.to_owned()),
        ("GITHUB_EVENT_PATH".to_owned(), format!("{DOCKER_FILES}/event.json")),
        ("RBIT_EVENT_PATH".to_owned(), format!("{DOCKER_FILES}/event.json")),
        ("RUNNER_TEMP".to_owned(), DOCKER_TEMP.to_owned()),
    ]);
    let mut env = job.process_env(step_env, extra);
    // The host's PATH means nothing in the image.
    env.remove("PATH");
    // The engine's env file, as for container steps: multi-line values travel in the
    // engine command's own environment, so a name that would change how the engine runs on
    // the host (`LD_*`, `GO*`, `DOCKER_*`, …) may not carry one.
    let env_file = job.private_dir().and_then(|dir| {
        crate::backend::container::EnvFile::write(&dir.join(format!("docker-{}.env", crate::client::idempotency_key())), &env)
    });
    let env_file = match env_file {
        Ok(env_file) => env_file,
        Err(error) => {
            let _ = files.collect();
            return job.fail(&format!("cannot write the action's environment: {error}"));
        }
    };
    // Named and labelled: stopping the engine's CLI leaves the container running, so it is
    // removed by name when the step ends, and by the janitor after a crash.
    let name = format!("rbit-{}-step-{index}-{}", job.spec.task_id, crate::client::idempotency_key().to_ascii_lowercase());
    let mut args: Vec<String> = vec!["run".into(), "--rm".into(), "--name".into(), name.clone()];
    args.extend(crate::backend::container::task_labels(job.spec.task_id, &job.runner_name));
    if let Some(network) = network {
        args.extend(["--network".into(), network]);
    }
    for (source, target) in [(&job.dirs.workspace, DOCKER_WORKSPACE), (&job.dirs.files, DOCKER_FILES), (&job.dirs.temp, DOCKER_TEMP)] {
        // A `,` would end the mount's source and add options of its own.
        if source.to_string_lossy().contains(',') {
            return job.fail(&format!("{} has a `,`, which `--mount` cannot take", source.display()));
        }
        args.extend(["--mount".into(), format!("type=bind,source={},target={target}", source.display())]);
    }
    args.extend(["--workdir".into(), DOCKER_WORKSPACE.into()]);
    args.extend(crate::backend::container::limits(&job.container));
    args.extend(["--env-file".into(), env_file.path().display().to_string()]);
    for name in env_file.names() {
        args.extend(["-e".into(), name]);
    }
    if let Some(entrypoint) = &run.entrypoint {
        args.extend(["--entrypoint".into(), entrypoint.clone()]);
    }
    args.push(image.to_owned());
    args.extend(run.args.iter().cloned());

    let mut command = process::command(program);
    command.args(&args);
    command.envs(env_file.inherited.iter().map(|(name, value)| (name, value)));
    let signal = job.stop_signal(timeout);
    let mut printed_outputs = BTreeMap::new();
    let exit = {
        let reporter = &job.reporter;
        let mut commands = crate::commands::Commands::new(job.step_debug);
        process::run_streaming(command, process::MAX_LINE_BYTES, signal.wait(), &crate::process::Stopper::Host, |line| {
            match commands.process(&line) {
                crate::commands::Line::Print(text) => reporter.log(&text),
                crate::commands::Line::AddMask(value) => reporter.add_mask(&value),
                crate::commands::Line::Output(name, value) => {
                    printed_outputs.insert(name, value);
                }
                // A Docker action has no `post` script to read state.
                crate::commands::Line::Nothing | crate::commands::Line::State(..) => {}
            }
        })
        .await
    };
    let mut remove = process::command(program);
    remove.args(["rm", "--force", &name]);
    let _ = process::capture(remove, None, Duration::from_mins(1)).await;
    drop(env_file);
    let written = files.collect();
    let outcome = match exit {
        Err(error) => job.fail(&format!("cannot start {}: {error}", program.display())),
        Ok(Exit::Stopped) => job.stopped(signal.reason()),
        Ok(Exit::Code(Some(0))) => Outcome::success(BTreeMap::new()),
        Ok(Exit::Code(Some(code))) => job.fail(&format!("Docker run failed with exit code {code}")),
        Ok(Exit::Code(None)) => job.fail("The process was killed by a signal."),
    };
    match written {
        Ok(written) => {
            job.reporter.set_summary(index, &written.summary);
            let mut outputs = printed_outputs;
            outputs.append(&mut super::run::apply(job, written));
            Outcome { outputs, ..outcome }
        }
        Err(error) => job.fail(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allow_list_decides_what_runs() {
        let repository = |owner: &str, repo: &str| ActionSource::Repository {
            owner: owner.into(),
            repo: repo.into(),
            path: String::new(),
            reference: "v1".into(),
        };
        let all = vec!["*".to_owned()];
        assert_eq!(refused(&all, &repository("actions", "setup-node")), None);
        let ours = vec!["actions/*".to_owned(), "My-Org/*".to_owned(), "docker://alpine:*".to_owned()];
        assert_eq!(refused(&ours, &repository("Actions", "setup-node")), None, "case does not matter");
        assert_eq!(refused(&ours, &repository("my-org", "tool")), None);
        assert!(refused(&ours, &repository("evil", "tool")).is_some());
        assert_eq!(refused(&ours, &ActionSource::Docker { image: "alpine:3.20".into() }), None);
        assert!(refused(&ours, &ActionSource::Docker { image: "ubuntu".into() }).is_some());
        assert_eq!(refused(&[], &ActionSource::Local { path: "x".into() }), None, "the workspace's own always run");
        assert!(refused(&[], &repository("actions", "setup-node")).is_some());
    }

    #[test]
    fn git_gets_no_credentials_and_one_scheme() {
        let env = git_env("https://github.com/actions/setup-node", None);
        let count: usize = env["GIT_CONFIG_COUNT"].parse().unwrap();
        let config: Vec<String> = (0..count)
            .map(|index| format!("{}={}", env[&format!("GIT_CONFIG_KEY_{index}")], env[&format!("GIT_CONFIG_VALUE_{index}")]))
            .collect();
        for expected in ["credential.helper=", "protocol.allow=never", "protocol.https.allow=always", "core.fsmonitor=false"] {
            assert!(config.contains(&expected.to_owned()), "{expected} in {config:?}");
        }
        assert!(!config.iter().any(|entry| entry.contains("extraheader")), "no token travels to the action source");
        assert_eq!(env["GIT_TERMINAL_PROMPT"], "0");
        let file = git_env("file:///srv/actions/x", None);
        assert!(file.values().any(|value| value == "protocol.file.allow"));
    }

    #[test]
    fn multi_line_values_are_passed_whole() {
        use crate::backend::container::EnvFile;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker.env");
        let env =
            BTreeMap::from([("INPUT_SCRIPT".to_owned(), "line one\nline two".to_owned()), ("INPUT_NAME".to_owned(), "plain".to_owned())]);
        let file = EnvFile::write(&path, &env).unwrap();
        assert_eq!(file.names(), ["INPUT_SCRIPT"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "INPUT_NAME=plain\n", "an env file cannot hold the line break");
        drop(file);
        for name in ["LD_PRELOAD", "LD_LIBRARY_PATH", "DOCKER_HOST", "GOMAXPROCS"] {
            let env = BTreeMap::from([(name.to_owned(), "x\n/__w/evil.so".to_owned())]);
            assert!(EnvFile::write(&path, &env).is_err(), "{name} must never reach the engine on the host");
        }
    }

    #[test]
    fn copies_are_fresh_and_leave_links_out() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        std::fs::create_dir_all(from.join("dist")).unwrap();
        std::fs::write(
            from.join("action.yml"),
            "runs: {using: node20, main: dist/index.js}
",
        )
        .unwrap();
        std::fs::write(from.join("dist/index.js"), "console.log(1)").unwrap();
        let temp = dir.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::write(from.join("run.sh"), "echo hi").unwrap();
            std::fs::set_permissions(from.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
            std::os::unix::fs::symlink("/etc/passwd", from.join("dist/passwd")).unwrap();
            // A container job planted `_actions` as a link to somewhere on the host.
            let elsewhere = dir.path().join("elsewhere");
            std::fs::create_dir_all(&elsewhere).unwrap();
            std::os::unix::fs::symlink(&elsewhere, temp.join("_actions")).unwrap();
        }
        let first = copy_into_task(&from, &temp).unwrap();
        let second = copy_into_task(&from, &temp).unwrap();
        assert_ne!(first, second, "each copy is a fresh directory");
        let copy = temp.join("_actions").join(&first);
        assert!(std::fs::symlink_metadata(temp.join("_actions")).unwrap().is_dir(), "a planted link is replaced, never followed");
        assert!(copy.join("dist/index.js").is_file() && !copy.join("dist/passwd").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert!(std::fs::read_dir(dir.path().join("elsewhere")).unwrap().next().is_none(), "nothing written through the link");
            assert_eq!(std::fs::metadata(copy.join("run.sh")).unwrap().permissions().mode() & 0o111, 0o111, "scripts stay executable");
        }
        assert!(matches!(read_manifest(&copy).unwrap().runs, Runs::Node(_)));
        assert!(read_manifest(dir.path()).unwrap_err().contains("no action.yml"));
        assert_eq!(complete(Path::new("/w/actions/o/r/abc")), Path::new("/w/actions/o/r/abc.complete"), "the mark is beside the tree");
    }

    #[cfg(unix)]
    #[test]
    fn manifests_and_dockerfiles_are_read_through_no_link() {
        let dir = tempfile::tempdir().unwrap();
        let action = dir.path().join("action");
        std::fs::create_dir_all(action.join("sub")).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", action.join("action.yml")).unwrap();
        assert!(read_manifest(&action).unwrap_err().contains("cannot read action.yml"), "a linked action.yml is refused");
        std::os::unix::fs::symlink("/etc", action.join("linked")).unwrap();
        std::fs::write(action.join("sub/Dockerfile"), "FROM alpine").unwrap();
        assert!(dockerfile(&action, "sub/Dockerfile").is_ok());
        assert!(dockerfile(&action, "linked/hostname").unwrap_err().contains("symbolic link"));
    }

    #[test]
    fn dockerfiles_stay_in_the_action_and_images_are_references() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Dockerfile"), "FROM alpine").unwrap();
        assert_eq!(dockerfile(dir.path(), "Dockerfile").unwrap(), dir.path().join("Dockerfile"));
        for outside in ["../Dockerfile", "/etc/Dockerfile", "./Dockerfile", "missing"] {
            assert!(dockerfile(dir.path(), outside).is_err(), "{outside}");
        }
        for image in ["alpine", "alpine:3.20", "ghcr.io/o/i:v1", "i@sha256:0123abcd", "docker.io/library/ubuntu:24.04"] {
            assert!(valid_image(image), "{image}");
        }
        for image in [
            "",
            "--privileged",
            "-v",
            "a b",
            "a;b",
            "/x",
            ":x",
            "a
b",
        ] {
            assert!(!valid_image(image), "{image:?}");
        }
    }

    #[test]
    fn docker_args_split_as_a_shell_would() {
        assert_eq!(split_args("echo hello  world"), ["echo", "hello", "world"]);
        assert_eq!(split_args("sh -c 'echo a b' \"x y\" ''"), ["sh", "-c", "echo a b", "x y", ""]);
        assert_eq!(split_args("   "), [] as [std::string::String; 0]);
    }

    #[test]
    fn pruning_keeps_what_was_used_lately() {
        let dir = tempfile::tempdir().unwrap();
        let actions = dir.path().join("actions");
        for sha in ["a", "b", "c"] {
            let cache = actions.join("o").join("r").join(sha);
            std::fs::create_dir_all(&cache).unwrap();
            std::fs::write(complete(&cache), sha).unwrap();
        }
        prune(&actions, 1);
        let left = std::fs::read_dir(actions.join("o/r")).unwrap().count();
        assert_eq!(left, 6, "nothing used within the hour goes");
        prune(&actions, 10);
    }
}
