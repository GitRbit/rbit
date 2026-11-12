//! `rbit task`: one issue, one branch, one worktree (agent plan step 5.5).
//!
//! * `start <issue>` takes the issue (`POST …/issues/{n}/take`: it fails with `conflict`
//!   when someone else holds it), fetches the base, and adds a Git worktree on a new branch
//!   `feature/<n>-<slug>` from that exact commit, beside the current checkout. `start
//!   <name>` does the same for a branch name without an issue. The branch name is the link:
//!   `rbit status` shows the issue a `feature/<n>-…` branch works on.
//! * `resume <name>` finds that worktree and reports its local state (head, uncommitted
//!   files) and the CI of its head on the server.
//! * `finish <name>` removes the worktree, and refuses while it has uncommitted changes.
//!   The branch stays.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::Subcommand;
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::Output,
    util::{encode, git, short, text},
};

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Take an issue (or name a branch) and start it in a new worktree.
    Start {
        /// An issue number, or a branch name for work without an issue.
        what: String,
        /// The branch to start from.
        #[arg(long, default_value = "main")]
        base: String,
        /// Where the worktree goes; beside the current checkout by default.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Where a task stands: its worktree, its local changes and its CI.
    Resume { name: String },
    /// Remove a task's worktree; refused while it has uncommitted changes.
    Finish { name: String },
}

/// `title` as a branch-name slug: lowercase words joined by `-`, at most 40 characters.
fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let mut out = String::new();
    for word in words {
        if out.len() + word.len() + 1 > 40 {
            break;
        }
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&word);
    }
    out
}

/// `git` in `dir`.
fn git_in(dir: &Path, args: &[&str]) -> Option<String> {
    let dir = dir.to_string_lossy();
    let mut all = vec!["-C", dir.as_ref()];
    all.extend_from_slice(args);
    git(&all)
}

/// The worktree that has branch `name` checked out.
fn worktree_of(name: &str) -> Option<PathBuf> {
    let listed = git(&["worktree", "list", "--porcelain"])?;
    let wanted = format!("branch refs/heads/{name}");
    let mut path = None;
    for line in listed.lines() {
        if let Some(found) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(found));
        } else if line == wanted {
            return path;
        }
    }
    None
}

/// Files with uncommitted changes in worktree `dir`.
fn dirty(dir: &Path) -> anyhow::Result<usize> {
    let status = git_in(dir, &["status", "--porcelain"]).context("cannot read the worktree's status")?;
    Ok(status.lines().filter(|line| !line.trim().is_empty()).count())
}

pub async fn task(api: &Api, repo: &Repo, remote: Option<&str>, command: TaskCommand, out: &Output) -> anyhow::Result<i32> {
    match command {
        TaskCommand::Start { what, base, dir } => start(api, repo, remote.unwrap_or("origin"), &what, &base, dir, out).await,
        TaskCommand::Resume { name } => resume(api, repo, &name, out).await,
        TaskCommand::Finish { name } => {
            let path = worktree_of(&name).ok_or_else(|| agent::outcome("not_found", format!("no worktree has `{name}` checked out"), None))?;
            let changed = dirty(&path)?;
            if changed > 0 {
                let message = format!("{} has {changed} uncommitted change(s); commit or discard them first", path.display());
                return Err(agent::outcome("conflict", message, Some(format!("git -C {} status", path.display()))));
            }
            git(&["worktree", "remove", &path.to_string_lossy()]).context("git worktree remove failed")?;
            super::runs::report(out, "rbit.task/1", json!({ "name": name, "path": path, "removed": true }), &format!("Removed {}", path.display()));
            Ok(0)
        }
    }
}

async fn start(api: &Api, repo: &Repo, remote: &str, what: &str, base: &str, dir: Option<PathBuf>, out: &Output) -> anyhow::Result<i32> {
    let top = git(&["rev-parse", "--show-toplevel"]).ok_or_else(|| agent::outcome("usage", "run `rbit task start` inside a clone", None))?;
    let (name, issue) = match what.parse::<u64>() {
        Ok(number) => {
            let taken = api.json(Method::POST, &format!("repos/{repo}/issues/{number}/take"), Some(&json!({}))).await?;
            (format!("feature/{number}-{}", slug(text(&taken, "title"))), Some(number))
        }
        Err(_) => (what.to_owned(), None),
    };
    if git(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_some() {
        let hint = Some(format!("rbit task resume {name}"));
        return Err(agent::outcome("conflict", format!("branch `{name}` exists already"), hint));
    }
    git(&["fetch", "--quiet", remote, base]).with_context(|| format!("cannot fetch `{base}` from `{remote}`"))?;
    let base_sha = git(&["rev-parse", "FETCH_HEAD"]).context("cannot read the fetched base")?;
    let top = PathBuf::from(top);
    let folder = format!("{}-{}", top.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), slug(&name));
    let path = dir.unwrap_or_else(|| top.parent().map_or_else(|| PathBuf::from(&folder), |parent| parent.join(&folder)));
    git(&["worktree", "add", "--quiet", "-b", &name, &path.to_string_lossy(), &base_sha]).context("git worktree add failed")?;
    let body = json!({ "name": name, "path": path, "base": base, "base_sha": base_sha, "issue": issue });
    super::runs::report(out, "rbit.task/1", body, &format!("{name} at {} from {base} {}", path.display(), short(&base_sha)));
    Ok(0)
}

async fn resume(api: &Api, repo: &Repo, name: &str, out: &Output) -> anyhow::Result<i32> {
    let path = worktree_of(name).ok_or_else(|| agent::outcome("not_found", format!("no worktree has `{name}` checked out"), None))?;
    let head = git_in(&path, &["rev-parse", "HEAD"]).unwrap_or_default();
    let changed = dirty(&path)?;
    let server: Value = api.get(&format!("repos/{repo}/agent/status?branch={}", encode(name))).await.unwrap_or(Value::Null);
    let pushed = server["head"]["sha"].as_str() == Some(head.as_str());
    let body = json!({
        "name": name,
        "path": path,
        "head": head,
        "uncommitted": changed,
        "pushed": pushed,
        "ci": server["ci"]["state"],
        "issue": server["issue"],
        "pr": server["pr"],
        "next": server["next"],
    });
    if out.json {
        agent::print("rbit.task/1", body);
    } else {
        println!("{name} at {}: {} @ {}", path.display(), if changed == 0 { "clean" } else { "uncommitted changes" }, short(&head));
        println!("  pushed: {pushed}; CI: {}", server["ci"]["state"].as_str().unwrap_or("unknown"));
        if let Some(next) = server["next"].as_str() {
            println!("  next: {next}");
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_become_short_branch_slugs() {
        assert_eq!(slug("Fix: `rbit status` 404 right after a push!"), "fix-rbit-status-404-right-after-a-push");
        assert_eq!(slug("feature/12-x"), "feature-12-x");
        assert!(slug(&"word ".repeat(30)).len() <= 40);
        assert_eq!(slug("¿¿??"), "");
    }
}
