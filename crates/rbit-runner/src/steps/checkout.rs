//! `rbit/checkout` and its `actions/checkout` aliases (§12.5.5).
//!
//! The checkout runs the `git` CLI on the runner host, writing into the task's workspace
//! (also for container jobs, whose images then need no `git`):
//!
//! 1. `init` and `remote add origin <url>` (or, for a directory checked out before,
//!    `clean -ffdx` and `reset --hard` when `clean` is set);
//! 2. `fetch` of the run's commit by SHA — or of `ref` — at `fetch-depth` (1 by default, 0
//!    for the whole history), with tags only when `fetch-tags` asks;
//! 3. `checkout --detach`, or a local branch when the ref is a branch;
//! 4. submodules and LFS objects when asked.
//!
//! The task token travels as an `Authorization` header in `http.<server>/.extraHeader`,
//! set through `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_n` / `GIT_CONFIG_VALUE_n`, so it is in
//! no URL and no argument. Scoped to this rbit instance's URL, it reaches the instance's
//! own submodules and nothing else. With `persist-credentials` (the default) the same
//! header is written into the checkout's `.git/config` so later steps can fetch and push;
//! the file is removed with the task.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use base64::Engine as _;
use rbit_workflow::{protocol::MAX_LOG_LINE_BYTES, spec::CheckoutSource};
use tokio::process::Command;

use super::{Inputs, Outcome};
use crate::{
    job::Job,
    process::{self, Exit},
};

/// The `with:` of a checkout that matter for its commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `ref`, or empty for the run's own commit.
    pub reference: String,
    /// 0 for the whole history.
    pub depth: u64,
    pub tags: bool,
    /// `false`, `true` or `recursive`.
    pub submodules: String,
    pub lfs: bool,
    pub clean: bool,
}

impl Options {
    pub fn from_inputs(inputs: &Inputs) -> Self {
        Self {
            reference: inputs.get("ref").trim().to_owned(),
            depth: inputs.get("fetch-depth").parse().unwrap_or(1),
            tags: inputs.flag("fetch-tags"),
            submodules: inputs.get("submodules").to_owned(),
            lfs: inputs.flag("lfs"),
            clean: inputs.flag("clean"),
        }
    }
}

/// The `git` argument lists of a checkout, in order. `existing` is whether the target
/// already holds a repository.
pub fn commands(source: &CheckoutSource, options: &Options, existing: bool) -> Vec<Vec<String>> {
    let args = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let mut commands = Vec::new();
    if existing {
        if options.clean {
            commands.push(args(&["clean", "-ffdx"]));
            commands.push(args(&["reset", "--hard", "HEAD"]));
        }
        commands.push(args(&["remote", "set-url", "origin", &source.url]));
    } else {
        commands.push(args(&["init", "--quiet"]));
        commands.push(args(&["remote", "add", "origin", &source.url]));
    }

    let by_sha = options.reference.is_empty() || is_object_id(&options.reference);
    let sha = if options.reference.is_empty() { source.sha.as_str() } else { options.reference.as_str() };
    let branch = if options.reference.is_empty() {
        source.reference.strip_prefix("refs/heads/")
    } else {
        options.reference.strip_prefix("refs/heads/")
    };
    let mut fetch = args(&["-c", "protocol.version=2", "fetch", if options.tags { "--tags" } else { "--no-tags" }, "--prune"]);
    fetch.push("--no-recurse-submodules".to_owned());
    if options.depth > 0 {
        fetch.push(format!("--depth={}", options.depth));
    }
    fetch.push("origin".to_owned());
    let revision = match (by_sha, branch) {
        (true, Some(branch)) => {
            fetch.push(format!("+{sha}:refs/remotes/origin/{branch}"));
            sha.to_owned()
        }
        (true, None) => {
            fetch.push(sha.to_owned());
            sha.to_owned()
        }
        (false, Some(branch)) => {
            fetch.push(format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"));
            format!("refs/remotes/origin/{branch}")
        }
        (false, None) => {
            fetch.push(options.reference.clone());
            "FETCH_HEAD".to_owned()
        }
    };
    commands.push(fetch);
    match branch {
        Some(branch) => commands.push(args(&["checkout", "--force", "-B", branch, &revision])),
        None => commands.push(args(&["checkout", "--force", "--detach", &revision])),
    }

    if matches!(options.submodules.as_str(), "true" | "recursive") {
        let recursive = options.submodules == "recursive";
        let mut sync = args(&["submodule", "sync"]);
        let mut update = args(&["-c", "protocol.version=2", "submodule", "update", "--init", "--force"]);
        if options.depth > 0 {
            update.push(format!("--depth={}", options.depth));
        }
        if recursive {
            sync.push("--recursive".to_owned());
            update.push("--recursive".to_owned());
        }
        commands.push(sync);
        commands.push(update);
    }
    if options.lfs {
        commands.push(args(&["lfs", "install", "--local"]));
        commands.push(args(&["lfs", "fetch", "origin", &revision]));
        commands.push(args(&["lfs", "checkout"]));
    }
    commands
}

fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The URL the token's header is scoped to: this rbit instance, with a trailing `/`.
pub fn server_prefix(source: &CheckoutSource) -> String {
    let base = source.url.strip_suffix(&format!("/{}", source.repository)).unwrap_or(&source.url);
    format!("{}/", base.trim_end_matches('/'))
}

/// `AUTHORIZATION: basic <x-access-token:token>`, as Git sends it.
pub fn authorization(token: &str) -> String {
    let credentials = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    format!("AUTHORIZATION: basic {credentials}")
}

/// The environment that gives `git` the header and keeps it from prompting or asking a
/// credential helper.
pub fn git_env(source: &CheckoutSource, token: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
        ("GCM_INTERACTIVE".to_owned(), "never".to_owned()),
        ("GIT_CONFIG_COUNT".to_owned(), "2".to_owned()),
        ("GIT_CONFIG_KEY_0".to_owned(), format!("http.{}.extraheader", server_prefix(source))),
        ("GIT_CONFIG_VALUE_0".to_owned(), authorization(token)),
        ("GIT_CONFIG_KEY_1".to_owned(), "credential.helper".to_owned()),
        ("GIT_CONFIG_VALUE_1".to_owned(), String::new()),
    ])
}

/// `path` inside `workspace`; `None` when it leaves it.
pub fn target(workspace: &Path, path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    relative.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir)).then(|| workspace.join(relative))
}

/// Runs a checkout step.
pub async fn checkout(job: &mut Job, inputs: &Inputs) -> Outcome {
    let Some(target) = target(&job.dirs.workspace, inputs.get("path")) else {
        return job.fail("`with.path` must stay inside the workspace");
    };
    let Some(git) = process::find_program("git") else {
        return job.fail("checkout needs `git` (2.30 or later) on the runner host");
    };
    if let Err(error) = std::fs::create_dir_all(&target) {
        return job.fail(&format!("cannot create {}: {error}", target.display()));
    }
    let options = Options::from_inputs(inputs);
    let source = job.spec.checkout.clone();
    let env = git_env(&source, job.spec.token.expose());
    let existing = target.join(".git").exists();
    job.reporter.log(&format!(
        "Checking out {} at {}",
        source.repository,
        if options.reference.is_empty() { &source.sha } else { &options.reference }
    ));
    for args in commands(&source, &options, existing) {
        job.reporter.log(&format!("[command]git {}", args.join(" ")));
        let mut command = Command::new(&git);
        command.args(&args).current_dir(&target).envs(&env);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().to_ascii_uppercase().starts_with("RBIT_RUNNER_") {
                command.env_remove(name);
            }
        }
        let cancel = job.reporter.cancellation().clone();
        let reporter = &job.reporter;
        match process::run_streaming(command, MAX_LOG_LINE_BYTES, async move { cancel.cancelled().await }, |line| reporter.log(&line)).await
        {
            Ok(Exit::Code(Some(0))) => {}
            Ok(Exit::Stopped) => return Outcome::cancelled(),
            Ok(Exit::Code(code)) => {
                let code = code.map_or_else(|| "a signal".to_owned(), |code| format!("exit code {code}"));
                return job.fail(&format!("`git {}` failed with {code}", args.first().map_or("", String::as_str)));
            }
            Err(error) => return job.fail(&format!("cannot run git: {error}")),
        }
    }
    if inputs.flag("persist-credentials") {
        let section = format!("\n[http \"{}\"]\n\textraheader = {}\n", server_prefix(&source), authorization(job.spec.token.expose()));
        let config = target.join(".git").join("config");
        let appended = std::fs::OpenOptions::new().append(true).open(&config).and_then(|mut file| {
            use std::io::Write as _;
            file.write_all(section.as_bytes())
        });
        if let Err(error) = appended {
            return job.fail(&format!("cannot persist the credentials in {}: {error}", config.display()));
        }
    }
    Outcome::success(BTreeMap::new())
}

#[cfg(test)]
mod tests {
    use rbit_workflow::{
        protocol::{StepStatus, TaskResult},
        spec::{Builtin, BuiltinKind, StepAction},
    };

    use super::*;
    use crate::job::tests::{execute, host_job, step};

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn source(reference: &str) -> CheckoutSource {
        CheckoutSource {
            url: "https://git.example.com/alice/app".into(),
            repository: "alice/app".into(),
            sha: SHA.into(),
            reference: reference.into(),
        }
    }

    fn options() -> Options {
        Options { reference: String::new(), depth: 1, tags: false, submodules: "false".into(), lfs: false, clean: true }
    }

    fn lines(commands: &[Vec<String>]) -> Vec<String> {
        commands.iter().map(|args| args.join(" ")).collect()
    }

    #[test]
    fn a_branch_is_fetched_by_sha_and_checked_out_as_a_branch() {
        assert_eq!(
            lines(&commands(&source("refs/heads/main"), &options(), false)),
            [
                "init --quiet".to_owned(),
                "remote add origin https://git.example.com/alice/app".to_owned(),
                format!(
                    "-c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +{SHA}:refs/remotes/origin/main"
                ),
                format!("checkout --force -B main {SHA}"),
            ]
        );
    }

    #[test]
    fn options_change_the_fetch() {
        let full = Options { depth: 0, tags: true, submodules: "recursive".into(), lfs: true, ..options() };
        let pull = lines(&commands(&source("refs/pull/3/head"), &full, true));
        assert_eq!(pull[0], "clean -ffdx");
        assert_eq!(pull[3], format!("-c protocol.version=2 fetch --tags --prune --no-recurse-submodules origin {SHA}"));
        assert_eq!(pull[4], format!("checkout --force --detach {SHA}"));
        assert_eq!(pull[5..7], ["submodule sync --recursive", "-c protocol.version=2 submodule update --init --force --recursive"]);
        assert_eq!(pull[7..], ["lfs install --local".to_owned(), format!("lfs fetch origin {SHA}"), "lfs checkout".to_owned()]);

        let tag = Options { reference: "v1.0".into(), ..options() };
        let tag = lines(&commands(&source("refs/heads/main"), &tag, false));
        assert!(tag[2].ends_with("origin v1.0") && tag[3] == "checkout --force --detach FETCH_HEAD", "{tag:?}");
        let other = Options { reference: "refs/heads/dev".into(), ..options() };
        let other = lines(&commands(&source("refs/heads/main"), &other, false));
        assert!(other[2].ends_with("+refs/heads/dev:refs/remotes/origin/dev"), "{other:?}");
        assert_eq!(other[3], "checkout --force -B dev refs/remotes/origin/dev");
    }

    #[test]
    fn the_token_is_in_the_environment_only() {
        let token = "rbat_FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
        let env = git_env(&source("refs/heads/main"), token);
        assert_eq!(env["GIT_CONFIG_KEY_0"], "http.https://git.example.com/.extraheader");
        let header = &env["GIT_CONFIG_VALUE_0"];
        let encoded = header.strip_prefix("AUTHORIZATION: basic ").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        assert_eq!(decoded, format!("x-access-token:{token}").as_bytes());
        for options in [options(), Options { submodules: "true".into(), lfs: true, ..options() }] {
            let all = lines(&commands(&source("refs/heads/main"), &options, false)).join(" ");
            assert!(!all.contains(token) && !all.contains(encoded), "{all}");
        }
        assert_eq!(
            server_prefix(&CheckoutSource { url: "https://example.com/rbit/alice/app".into(), ..source("refs/heads/main") }),
            "https://example.com/rbit/"
        );
        assert!(target(Path::new("/w"), "../other").is_none() && target(Path::new("/w"), "/etc").is_none());
        assert_eq!(target(Path::new("/w"), "sub/dir"), Some(Path::new("/w").join("sub/dir")));
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn checkout_step(with: &[(&str, &str)]) -> rbit_workflow::spec::StepTemplate {
        let mut template = step("");
        template.action = StepAction::Uses(Builtin {
            uses: "actions/checkout@v4".into(),
            kind: BuiltinKind::Checkout,
            version: 1,
            with: with.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
        });
        template
    }

    #[tokio::test]
    async fn a_local_repository_is_checked_out() {
        if process::find_program("git").is_none() {
            return;
        }
        let origin = tempfile::tempdir().unwrap();
        let repo = origin.path().join("alice").join("app");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--quiet"]);
        std::fs::write(repo.join("a.txt"), "one").unwrap();
        git(&repo, &["add", "a.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "one"]);
        std::fs::write(repo.join("a.txt"), "two").unwrap();
        git(&repo, &["commit", "--quiet", "-am", "two"]);
        let sha = git(&repo, &["rev-parse", "HEAD"]);

        let url = format!("file://{}{}", if cfg!(windows) { "/" } else { "" }, repo.to_string_lossy().replace('\\', "/"));
        let mut spec = host_job(vec![checkout_step(&[]), checkout_step(&[("path", "../escape")])]);
        spec.checkout =
            CheckoutSource { url: url.clone(), repository: "alice/app".into(), sha: sha.clone(), reference: "refs/heads/main".into() };
        let token = spec.token.expose().to_owned();
        let (result, reporter, work) = execute(spec).await;
        let text = reporter.lines().join("\n");
        assert_eq!(result, TaskResult::Failure, "{text}");
        assert_eq!(
            reporter.state().steps.iter().map(|step| step.status).collect::<Vec<_>>(),
            [StepStatus::Success, StepStatus::Failure],
            "{text}"
        );
        assert!(text.contains("must stay inside the workspace"));

        let workspace = crate::task::task_dir(work.path(), rbit_workflow::protocol::TaskId(1)).join("workspace");
        assert_eq!(std::fs::read_to_string(workspace.join("a.txt")).unwrap(), "two");
        assert_eq!(git(&workspace, &["rev-parse", "HEAD"]), sha);
        assert_eq!(git(&workspace, &["branch", "--show-current"]), "main");
        assert_eq!(git(&workspace, &["rev-list", "--count", "HEAD"]), "1", "fetch-depth defaults to 1");
        let config = std::fs::read_to_string(workspace.join(".git/config")).unwrap();
        assert!(config.contains(&format!(
            "[http \"{}\"]",
            server_prefix(&CheckoutSource { url, repository: "alice/app".into(), sha, reference: String::new() })
        )));
        assert!(config.contains(&authorization(&token)), "persist-credentials writes the header");
        assert!(!text.contains(&token), "{text}");
    }
}
