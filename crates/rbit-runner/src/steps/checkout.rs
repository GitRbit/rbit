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
//!
//! `git` may use only HTTPS and the source's own scheme (plain HTTP when the instance is
//! reached that way; `file` for `rbit-runner exec`'s local repository), also for submodules: a repository's `.gitmodules` chooses their URLs, and on the host
//! `ssh://`, `file://` or a local path would reach the runner user's SSH keys or other
//! repositories on the runner. `~/.netrc` is still read for HTTPS hosts; keep runner hosts
//! free of credentials a job should not use.
//!
//! A container job can write the workspace, and `git` reads a repository's own
//! configuration — hooks, `core.fsmonitor` — and runs it on the host, so in a container job
//! the checkout uses no `.git` it finds: it removes every one under the path (a submodule's
//! included, since `git submodule update` would otherwise run a planted `.git`'s hooks on the
//! host) and starts from step 1 for a new directory, then runs `clean -ffdx` when `clean` is
//! set. Whatever the backend, checkout's `git` points its hooks at an empty private directory
//! no container mounts and disables `fsmonitor`, so no repository hook or command runs on the
//! host. A `path` that goes through a symbolic link is refused.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use base64::Engine as _;
use rbit_workflow::spec::CheckoutSource;
use tokio::process::Command;

use super::{Inputs, Outcome};
use crate::{
    backend::Backend,
    job::Job,
    process::{self, Exit, Stopper},
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

/// The environment that gives `git` the header, keeps it from prompting or asking a
/// credential helper, lets it use only HTTPS and the source URL's own scheme (submodules
/// included), and keeps it from running a repository's hooks or `fsmonitor` command on the
/// host: `hooks_path` is an empty directory no container mounts.
pub fn git_env(source: &CheckoutSource, token: &str, hooks_path: Option<&Path>) -> BTreeMap<String, String> {
    let mut config = vec![
        (format!("http.{}.extraheader", server_prefix(source)), authorization(token)),
        ("credential.helper".to_owned(), String::new()),
        ("protocol.allow".to_owned(), "never".to_owned()),
        ("protocol.https.allow".to_owned(), "always".to_owned()),
        // A job controls the workspace; host `git` must run no hook or `fsmonitor` command a
        // `.git` there carries. A command-line value wins over any repository config.
        ("core.fsmonitor".to_owned(), "false".to_owned()),
    ];
    if let Some(path) = hooks_path {
        config.push(("core.hooksPath".to_owned(), path.display().to_string()));
    }
    if let Some((scheme, _)) = source.url.split_once("://").filter(|(scheme, _)| matches!(*scheme, "http" | "file")) {
        config.push((format!("protocol.{scheme}.allow"), "always".to_owned()));
    }
    let mut env = BTreeMap::from([
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
        ("GCM_INTERACTIVE".to_owned(), "never".to_owned()),
        ("GIT_CONFIG_COUNT".to_owned(), config.len().to_string()),
    ]);
    for (index, (key, value)) in config.into_iter().enumerate() {
        env.insert(format!("GIT_CONFIG_KEY_{index}"), key);
        env.insert(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    env
}

/// `path` inside `workspace`; `None` when it leaves it or goes through a symbolic link.
pub fn target(workspace: &Path, path: &str) -> Option<PathBuf> {
    let relative = Path::new(path);
    (relative.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        && !crate::untrusted::crosses_link(workspace, relative))
    .then(|| workspace.join(relative))
}

/// Removes every `.git` under `root`, without following symbolic links; returns how many
/// were removed. A container job can plant a `.git` deep in the workspace (a submodule's
/// path), and `git submodule update` on the host would run that `.git`'s hooks, so the whole
/// tree is cleared before a container checkout, not only the top (§12.9).
fn discard_git_tree(root: &Path) -> std::io::Result<usize> {
    let mut removed = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if discard_git(&dir)? {
            removed += 1;
        }
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    // `.git` is gone; descend only into real directories, never a link the job
                    // may have planted to reach outside the workspace.
                    if entry.file_name() != ".git" && std::fs::symlink_metadata(entry.path())?.is_dir() {
                        stack.push(entry.path());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(removed)
}

/// Removes `target`'s `.git`, whatever it is; whether there was one.
pub fn discard_git(target: &Path) -> std::io::Result<bool> {
    let git = target.join(".git");
    match std::fs::symlink_metadata(&git) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(&git).map(|()| true),
        Ok(_) => std::fs::remove_file(&git).map(|()| true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Runs a checkout step.
pub async fn checkout(job: &mut Job, inputs: &Inputs, timeout: Option<Duration>) -> Outcome {
    let Some(target) = target(&job.dirs.workspace, inputs.get("path")) else {
        return job.fail("`with.path` must stay inside the workspace and not go through a symbolic link");
    };
    let Some(git) = process::find_program("git") else {
        return job.fail("checkout needs `git` (2.31 or later) on the runner host");
    };
    // An older git ignores the GIT_CONFIG_* variables, and with them the task token and
    // the protocols it may use.
    if let Ok((version, text)) = crate::check::git_version().await
        && version < crate::check::MIN_GIT
    {
        let (major, minor) = crate::check::MIN_GIT;
        return job.fail(&format!("checkout needs git {major}.{minor} or later on the runner host, not {text}"));
    }
    if let Err(error) = std::fs::create_dir_all(&target) {
        return job.fail(&format!("cannot create {}: {error}", target.display()));
    }
    let options = Options::from_inputs(inputs);
    let source = job.spec.checkout.clone();
    // Host `git`'s hooks point at a private directory no container mounts, so none can run.
    let hooks = job.private_dir().ok();
    let env = git_env(&source, job.spec.token.expose(), hooks.as_deref());
    let mut existing = target.join(".git").exists();
    let mut discarded = false;
    if matches!(job.backend, Backend::Container(_)) {
        match discard_git_tree(&target) {
            Ok(0) => {}
            Ok(_) => {
                job.reporter.log("The job's container can change `.git`, so the checkout starts a new repository");
                (existing, discarded) = (false, true);
            }
            Err(error) => return job.fail(&format!("cannot remove the earlier checkout's `.git`: {error}")),
        }
    }
    job.reporter.log(&format!(
        "Checking out {} at {}",
        source.repository,
        if options.reference.is_empty() { &source.sha } else { &options.reference }
    ));
    let mut list = commands(&source, &options, existing);
    if discarded && options.clean {
        list.push(["clean", "-ffdx"].map(str::to_owned).to_vec());
    }
    for args in list {
        job.reporter.log(&format!("[command]git {}", args.join(" ")));
        let mut command = Command::new(&git);
        command.args(&args).current_dir(&target).envs(&env);
        process::isolate(&mut command);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().to_ascii_uppercase().starts_with("RBIT_RUNNER_") {
                command.env_remove(name);
            }
        }
        let signal = job.stop_signal(timeout);
        let reporter = &job.reporter;
        match process::run_streaming(command, process::MAX_LINE_BYTES, signal.wait(), &Stopper::Host, |line| reporter.log(&line)).await {
            Ok(Exit::Code(Some(0))) => {}
            Ok(Exit::Stopped) => return job.stopped(signal.reason()),
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
        let env = git_env(&source("refs/heads/main"), token, None);
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

        // Only HTTPS, and HTTP only for an instance reached over HTTP: no SSH keys, no local
        // repositories through a submodule's URL.
        let config = |env: &BTreeMap<String, String>| {
            let count: usize = env["GIT_CONFIG_COUNT"].parse().unwrap();
            (0..count)
                .map(|index| format!("{}={}", env[&format!("GIT_CONFIG_KEY_{index}")], env[&format!("GIT_CONFIG_VALUE_{index}")]))
                .collect::<Vec<_>>()
        };
        let https = config(&env);
        assert!(https.contains(&"protocol.allow=never".to_owned()) && https.contains(&"protocol.https.allow=always".to_owned()));
        assert!(!https.iter().any(|entry| entry.starts_with("protocol.http.")), "{https:?}");
        let plain = git_env(&CheckoutSource { url: "http://localhost:3000/alice/app".into(), ..source("refs/heads/main") }, token, None);
        assert!(config(&plain).contains(&"protocol.http.allow=always".to_owned()));
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

    /// A container job can leave a `.git` that runs commands when host `git` reads it, or
    /// a link out of the workspace where the checkout would write.
    #[cfg(unix)]
    #[test]
    fn a_job_written_git_or_link_is_not_used() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let repo = workspace.path().join("app");
        std::fs::create_dir_all(repo.join(".git/hooks")).unwrap();
        std::fs::write(
            repo.join(".git/config"),
            "[core]
	fsmonitor = touch /tmp/pwned
",
        )
        .unwrap();
        std::fs::write(repo.join("kept.txt"), "work").unwrap();
        assert!(discard_git(&repo).unwrap());
        assert!(!repo.join(".git").exists() && repo.join("kept.txt").exists());
        assert!(!discard_git(&repo).unwrap(), "nothing left to remove");

        let elsewhere = outside.path().join(".git");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, repo.join(".git")).unwrap();
        assert!(discard_git(&repo).unwrap());
        assert!(elsewhere.exists(), "a linked `.git` is unlinked, not emptied");

        std::os::unix::fs::symlink(outside.path(), workspace.path().join("out")).unwrap();
        assert_eq!(target(workspace.path(), "out"), None);
        assert_eq!(target(workspace.path(), "out/sub"), None);
        assert_eq!(target(workspace.path(), "app"), Some(repo));
    }

    /// A container job can plant a `.git` in a submodule's path whose hooks host `git` would
    /// run; the checkout clears every `.git` under the path, but not one past a link.
    #[cfg(unix)]
    #[test]
    fn a_nested_git_is_cleared_before_checkout() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let repo = workspace.path().join("app");
        std::fs::create_dir_all(repo.join("sub/.git/hooks")).unwrap();
        std::fs::write(
            repo.join("sub/.git/hooks/post-checkout"),
            "#!/bin/sh
",
        )
        .unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join("keep.txt"), "work").unwrap();
        std::fs::create_dir(outside.path().join(".git")).unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.join("out")).unwrap();

        assert_eq!(discard_git_tree(&repo).unwrap(), 2, "the top and the submodule `.git`");
        assert!(!repo.join(".git").exists() && !repo.join("sub/.git").exists());
        assert!(repo.join("keep.txt").exists() && repo.join("sub").exists());
        assert!(outside.path().join(".git").exists(), "a `.git` past a symbolic link is left alone");
        assert_eq!(discard_git_tree(&repo).unwrap(), 0, "nothing left to clear");
    }
}
