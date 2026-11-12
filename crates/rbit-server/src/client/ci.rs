//! `rbit ci run`: tests a commit that is on no branch and in no pull request (agent plan
//! steps 3.2 and 3.3).
//!
//! By default the commit is built from the working tree as it is, tracked and untracked
//! files alike (what `.gitignore` leaves out stays out), through a temporary index: `HEAD`,
//! the real index and the files are never touched. `--commit` tests an existing commit
//! instead. The commit is pushed to `refs/scratch/<login>/<id>`, which the server keeps
//! for a day and never lists as a branch, and the run is then watched by its commit, like
//! `rbit run watch --sha`. `--jobs a,b` adds an `Rbit-CI-Jobs: a, b` trailer to a built
//! commit, so only those jobs and the jobs they need run.

use std::{
    io::Write as _,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context as _, bail};
use clap::Args;
use serde_json::json;

use super::{
    Api, Repo, agent,
    commands::Output,
    status::{CommitWait, watch_commit},
    util::{git, short, text},
};

/// The options of `rbit ci run`.
#[derive(Debug, Args)]
pub struct CiRun {
    /// Only these jobs and the jobs they need (comma-separated job ids).
    #[arg(long, value_delimiter = ',')]
    jobs: Vec<String>,
    /// Test this existing commit instead of building one from the working tree.
    #[arg(long, conflicts_with = "jobs")]
    commit: Option<String>,
    /// Push and print the commit, without waiting for the run.
    #[arg(long)]
    no_watch: bool,
    /// Stop watching after this many seconds (exit 8).
    #[arg(long, default_value_t = 3600)]
    timeout: u64,
}

/// `git` with `envs`, in the current directory; its trimmed output, or an error with its
/// standard error.
fn git_with(args: &[&str], envs: &[(&str, &std::ffi::OsStr)], input: Option<&str>) -> anyhow::Result<String> {
    let mut command = Command::new("git");
    command.args(args).envs(envs.iter().copied()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = command.spawn().context("cannot run git")?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The message of a commit built from the working tree on top of `base`.
fn message(base: Option<&str>, branch: Option<&str>, jobs: &[String]) -> String {
    let on = match (branch, base) {
        (Some(branch), Some(base)) => format!("{branch} at {}", short(base)),
        (None, Some(base)) => short(base).to_owned(),
        (_, None) => "an empty history".to_owned(),
    };
    let mut message = format!("rbit ci run: the working tree on {on}\n");
    if !jobs.is_empty() {
        message.push_str(&format!("\nRbit-CI-Jobs: {}\n", jobs.join(", ")));
    }
    message
}

/// An index file deleted when dropped.
struct TemporaryIndex(PathBuf);

impl Drop for TemporaryIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A commit of the working tree, made through a temporary index.
fn working_tree_commit(jobs: &[String]) -> anyhow::Result<String> {
    let top = git(&["rev-parse", "--show-toplevel"]).ok_or_else(|| agent::outcome("usage", "run `rbit ci run` inside a clone", None))?;
    let base = git(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let index = TemporaryIndex(PathBuf::from(git_with(
        &["rev-parse", "--path-format=absolute", "--git-path", &format!("rbit-ci-index-{}", std::process::id())],
        &[],
        None,
    )?));
    let envs = [("GIT_INDEX_FILE", index.0.as_os_str())];
    match &base {
        Some(base) => git_with(&["read-tree", base], &envs, None)?,
        None => git_with(&["read-tree", "--empty"], &envs, None)?,
    };
    git_with(&["-C", &top, "add", "--all", "--", "."], &envs, None)?;
    let tree = git_with(&["write-tree"], &envs, None)?;
    let mut args = vec!["commit-tree", tree.as_str(), "-F", "-"];
    if let Some(base) = &base {
        args.extend(["-p", base.as_str()]);
    }
    git_with(&args, &[], Some(&message(base.as_deref(), branch.as_deref(), jobs)))
}

/// `rbit ci run`.
pub async fn run(api: &Api, repo: &Repo, remote: Option<&str>, options: CiRun, out: &Output) -> anyhow::Result<i32> {
    let sha = match &options.commit {
        Some(commit) => git(&["rev-parse", "--verify", "--quiet", &format!("{commit}^{{commit}}")])
            .ok_or_else(|| agent::outcome("usage", format!("`{commit}` is not a commit of this clone"), None))?,
        None => working_tree_commit(&options.jobs)?,
    };
    let me = api.get("user").await?;
    let login = text(&me, "login");
    if login.is_empty() {
        bail!("the server did not say who you are; run `rbit auth login`");
    }
    let reference = format!("refs/scratch/{login}/{}", short(&sha));
    let remote = remote.unwrap_or("origin");
    git_with(&["push", "--quiet", remote, &format!("{sha}:{reference}")], &[], None)?;
    if options.no_watch {
        if out.json {
            agent::print("rbit.ci-run/1", json!({ "sha": sha, "ref": reference, "jobs": options.jobs }));
        } else {
            println!("{sha}\npushed to {reference}; watch it with `rbit run watch --sha {sha}`");
        }
        return Ok(0);
    }
    eprintln!("testing {} as {reference}", short(&sha));
    let wait = CommitWait { sha: Some(sha), branch: None, exit_status: true, timeout: options.timeout };
    watch_commit(api, repo, wait, out).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_base_and_the_jobs() {
        let base = "4c1e9a0b".repeat(5);
        assert_eq!(message(Some(&base), Some("main"), &[]), "rbit ci run: the working tree on main at 4c1e9a0b4c1e\n");
        let jobs = ["lint".to_owned(), "tests".to_owned()];
        assert_eq!(message(Some(&base), None, &jobs), "rbit ci run: the working tree on 4c1e9a0b4c1e\n\nRbit-CI-Jobs: lint, tests\n");
        assert_eq!(message(None, Some("main"), &[]), "rbit ci run: the working tree on an empty history\n");
    }
}
