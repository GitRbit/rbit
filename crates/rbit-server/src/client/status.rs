//! `rbit status` and `rbit run watch` for a commit: one call for "where am I", and a wait
//! that wakes on the first change that matters (agent plan steps 1.6 and 1.7).
//!
//! `status` reads the server's `rbit.status/1` for a branch (the current one by default)
//! and adds whether the local checkout's `HEAD` is that commit, so a push that did not
//! happen is not mistaken for one that did. `watch` long-polls the server: it returns as
//! soon as a job fails (exit 1), when the runs succeed (0), when the branch moves on
//! (`head_changed`, exit 5), when no run will come (`none_expected`, 0), when nobody can
//! say why a job failed (`indeterminate`, 9), when a job waits for labels no runner of the
//! repository has (`no_runner`, 8: it would never start), or at `--timeout` (`timeout`, 8).

use std::time::Duration;

use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::Output,
    util::{encode, git, short, text},
};

/// The current branch of the checkout.
fn current_branch() -> Option<String> {
    git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|branch| branch != "HEAD")
}

pub async fn status(api: &Api, repo: &Repo, branch: Option<String>, out: &Output) -> anyhow::Result<i32> {
    let branch = branch.or_else(current_branch);
    let query = branch.as_deref().map(|branch| format!("?branch={}", encode(branch))).unwrap_or_default();
    let mut answer = api.get(&format!("repos/{repo}/agent/status{query}")).await?;
    let server_head = text(&answer["head"], "sha").to_owned();
    if let Some(local) = git(&["rev-parse", "HEAD"]) {
        answer["local"] = json!({ "sha": local, "pushed": local == server_head });
    }
    if out.json {
        let mut body = answer;
        if let Some(object) = body.as_object_mut() {
            object.remove("schema");
        }
        agent::print("rbit.status/1", body);
        return Ok(0);
    }
    let ci = &answer["ci"];
    println!("{} @ {} on {}", text(&answer, "branch"), short(&server_head), text(&answer, "repo"));
    if answer["local"]["pushed"] == false {
        println!("  local HEAD {} is not what the server has", short(text(&answer["local"], "sha")));
    }
    if let Some(divergence) = answer["head"]["divergence"].as_object() {
        println!("  {} ahead, {} behind {}", divergence["ahead"], divergence["behind"], text(&answer["head"], "base"));
    }
    let reason = ci["reason"].as_str().map(|reason| format!(" ({reason})")).unwrap_or_default();
    println!("  CI: {}{reason}", text(ci, "state"));
    for failed in ci["failed"].as_array().into_iter().flatten() {
        let termination = &failed["termination"];
        println!("    {} failed: {} {}", text(failed, "job"), text(termination, "reason"), termination["step_name"].as_str().unwrap_or(""));
    }
    println!("  base {}: {}", text(&answer["base"], "branch"), answer["base"]["ci"].as_str().unwrap_or("no runs"));
    if let Some(pr) = answer["pr"].as_object() {
        println!("  pull request #{}{}", pr["number"], if pr["draft"] == true { " (draft)" } else { "" });
    }
    if let Some(next) = answer["next"].as_str() {
        println!("  next: {next}");
    }
    Ok(0)
}

/// The full id of `sha` (a prefix is fine): from the checkout, else from the server.
async fn full_sha(api: &Api, repo: &Repo, sha: &str) -> anyhow::Result<String> {
    if let Some(full) = git(&["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")]) {
        return Ok(full);
    }
    let commit = api.get(&format!("repos/{repo}/commits/{}", encode(sha))).await?;
    Ok(text(&commit, "sha").to_owned())
}

/// How `watch_commit` waits.
pub struct CommitWait {
    pub sha: Option<String>,
    pub branch: Option<String>,
    pub exit_status: bool,
    pub timeout: u64,
}

/// `run watch` for a commit: long-polls until the CI of the commit settles or fails.
pub async fn watch_commit(api: &Api, repo: &Repo, wait: CommitWait, out: &Output) -> anyhow::Result<i32> {
    // Watching `HEAD` follows the current branch, so a newer push ends the watch.
    let branch = if wait.sha.is_none() { wait.branch.or_else(current_branch) } else { wait.branch };
    let sha = match &wait.sha {
        Some(sha) => full_sha(api, repo, sha).await?,
        None => git(&["rev-parse", "HEAD"]).ok_or_else(|| agent::outcome("usage", "pass --sha or run inside a clone", None))?,
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait.timeout);
    let mut cursor: Option<String> = None;
    let mut last_state = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now()).as_secs().clamp(1, 25);
        let mut query = format!("sha={sha}&timeout={remaining}");
        if let Some(branch) = &branch {
            query = format!("{query}&branch={}", encode(branch));
        }
        if let Some(cursor) = &cursor {
            query = format!("{query}&cursor={}", encode(cursor));
        }
        let answer = api.get(&format!("repos/{repo}/agent/watch?{query}")).await?;
        let state = text(&answer, "state").to_owned();
        if !out.json && state != last_state {
            let jobs = &answer["jobs"];
            eprintln!(
                "{state:<14} {} ok, {} failed, {} running, {} queued",
                jobs["success"], jobs["failure"], jobs["running"], jobs["queued"]
            );
            last_state.clone_from(&state);
        }
        if let Some(moved) = answer["head_changed"].as_object() {
            let newer = moved.get("sha").and_then(Value::as_str).unwrap_or_default().to_owned();
            let message =
                format!("{} moved on to {} while {} was watched", branch.as_deref().unwrap_or("the branch"), short(&newer), short(&sha));
            return Err(agent::outcome("head_changed", message, Some(format!("rbit run watch --sha {newer}"))));
        }
        if let Some(labels) = answer["waits_for"].as_array() {
            let labels: Vec<&str> = labels.iter().filter_map(Value::as_str).collect();
            let message = format!("a job of {} waits for `{}`, which no runner of this repository has", short(&sha), labels.join(", "));
            return Err(agent::outcome("no_runner", message, None));
        }
        let early_failure = matches!(state.as_str(), "failure" | "indeterminate");
        if answer["done"] == true || early_failure {
            return Ok(finish(&answer, &state, wait.exit_status, out));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(agent::outcome(
                "timeout",
                format!("the CI of {} did not settle in time ({state})", short(&sha)),
                Some(format!("rbit run watch --sha {sha}")),
            ));
        }
        cursor = answer["cursor"].as_str().map(str::to_owned);
    }
}

/// The last answer of a watch, printed, and its exit code.
fn finish(answer: &Value, state: &str, exit_status: bool, out: &Output) -> i32 {
    if out.json {
        println!("{answer}");
    } else {
        println!("{state}: {}", text(answer, "sha"));
        for failed in answer["failed"].as_array().into_iter().flatten() {
            let termination = &failed["termination"];
            println!(
                "  {} (job {}): {} {}",
                text(failed, "job"),
                failed["job_id"],
                text(termination, "reason"),
                termination["step_name"].as_str().unwrap_or("")
            );
        }
    }
    match state {
        "indeterminate" => 9,
        "failure" | "cancelled" if exit_status => 1,
        _ => 0,
    }
}
