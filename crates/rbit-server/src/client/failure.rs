//! `rbit run failure` and `rbit run log`: what failed, without reading whole logs.
//!
//! * `run failure RUN` prints the server's failure digest (`rbit.failure/1`); `--sha` (or
//!   `HEAD`) gathers the digests of every failed run of a commit into one object
//!   (`rbit.failures/1`) with the commit's overall `state`.
//! * `run log JOB --step N` prints that step's lines, the tail by default.
//!
//! Exit codes: 0 nothing failed, 1 something failed, 8 a run of the commit has not finished
//! (or none has started), 9 a failure whose log is incomplete or whose cause nobody
//! recorded (a lost runner): the answer cannot be trusted as the whole story.

use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::Output,
    util::{encode, git, short, text},
};

/// Whether a digest holds a failure whose account is incomplete.
fn untrusted(digest: &Value) -> bool {
    digest["jobs"].as_array().into_iter().flatten().any(|job| {
        let termination = &job["termination"];
        termination["log_complete"] == false || matches!(termination["reason"].as_str(), Some("runner_lost" | "unknown"))
    })
}

fn exit_for(failed: bool, untrusted: bool) -> i32 {
    match (failed, untrusted) {
        (true, true) => 9,
        (true, false) => 1,
        (false, _) => 0,
    }
}

pub async fn failure(api: &Api, repo: &Repo, run: Option<i64>, sha: Option<String>, out: &Output) -> anyhow::Result<i32> {
    if let Some(run) = run {
        let digest = api.get(&format!("repos/{repo}/actions/runs/{run}/failure")).await?;
        let failed = digest["jobs"].as_array().is_some_and(|jobs| !jobs.is_empty());
        show(&digest, out);
        return Ok(exit_for(failed, untrusted(&digest)));
    }
    let Some(sha) = sha.or_else(|| git(&["rev-parse", "HEAD"])) else {
        return Err(agent::outcome("usage", "pass a run id or --sha, or run inside a clone", None));
    };
    let listed = api.get(&format!("repos/{repo}/actions/runs?head_sha={}&per_page=50", encode(&sha))).await?;
    let runs = listed["workflow_runs"].as_array().cloned().unwrap_or_default();
    if runs.is_empty() {
        let message = format!("no run for {} yet: it has not started, or no workflow applies to it", short(&sha));
        return Err(agent::outcome("pending", message, Some(format!("rbit run watch --sha {sha}"))));
    }
    let mut digests = Vec::new();
    for run in runs.iter().filter(|run| run["status"] == "completed" && run["conclusion"] != "success") {
        digests.push(api.get(&format!("repos/{repo}/actions/runs/{}/failure", run["id"])).await?);
    }
    let pending = runs.iter().any(|run| run["status"] != "completed");
    let state = if !digests.is_empty() {
        "failure"
    } else if pending {
        "pending"
    } else {
        "success"
    };
    let distrust = digests.iter().any(untrusted);
    if out.json {
        agent::print("rbit.failures/1", json!({ "sha": sha, "state": state, "runs": digests }));
    } else {
        for digest in &digests {
            show(digest, out);
        }
        if digests.is_empty() {
            println!("{state}: no failed run for {}", short(&sha));
        }
    }
    Ok(match state {
        "failure" => exit_for(true, distrust),
        "pending" => 8,
        _ => 0,
    })
}

/// A digest: as it is with `--json`, readable otherwise.
fn show(digest: &Value, out: &Output) {
    if out.json {
        println!("{digest}");
        return;
    }
    let jobs = digest["jobs"].as_array().cloned().unwrap_or_default();
    println!("run {} (#{}) at {}: {} failed job(s)", digest["run"], digest["number"], short(text(digest, "sha")), jobs.len());
    for job in &jobs {
        let termination = &job["termination"];
        let step = termination["step_name"].as_str().map(|name| format!(" at step {} \"{name}\"", termination["step"])).unwrap_or_default();
        let code = termination["exit_code"].as_i64().map(|code| format!(", exit {code}")).unwrap_or_default();
        let incomplete = if termination["log_complete"] == false { " (log incomplete)" } else { "" };
        let base = if job["also_failing_on_base"] == true { ", also failing on the base" } else { "" };
        println!(
            "  {} (job {}): {}{step}{code}{incomplete} [{}{base}]",
            text(job, "job"),
            job["job_id"],
            text(termination, "reason"),
            text(job, "seen")
        );
        for test in job["tests"]["items"].as_array().into_iter().flatten() {
            let place = test["path"].as_str().map(|path| format!(" ({path}:{})", test["line"])).unwrap_or_default();
            let flaky = if test["flaky"] == true { " [flaky]" } else { "" };
            println!("    test {}{place}{flaky}: {}", text(test, "id"), text(test, "message"));
        }
        for annotation in job["annotations"].as_array().into_iter().flatten() {
            let place = match (annotation["path"].as_str(), annotation["line"].as_u64()) {
                (Some(path), Some(line)) => format!("{path}:{line}: "),
                (Some(path), None) => format!("{path}: "),
                _ => String::new(),
            };
            println!("    {place}{}", text(annotation, "message"));
        }
        if let Some(excerpt) = job["excerpt"].as_object() {
            for line in excerpt.get("lines").and_then(Value::as_array).into_iter().flatten() {
                println!("    | {}", line.as_str().unwrap_or(""));
            }
            if let Some(more) = excerpt.get("more").and_then(Value::as_str) {
                println!("    (showing {} of {} lines; more: {more})", excerpt["shown"], excerpt["total"]);
            }
        }
    }
    for resolved in digest["resolved"].as_array().into_iter().flatten() {
        println!("  resolved since the previous run: {}", resolved.as_str().unwrap_or(""));
    }
}

/// What `run log` reads.
pub struct LogRequest {
    pub job: i64,
    pub step: i64,
    pub tail: Option<u32>,
    pub from: Option<u64>,
    pub lines: Option<u32>,
    pub attempt: Option<i64>,
    pub raw: bool,
}

pub async fn log(api: &Api, repo: &Repo, request: &LogRequest, out: &Output) -> anyhow::Result<i32> {
    let mut query = Vec::new();
    for (key, value) in [("tail", request.tail.map(u64::from)), ("from", request.from), ("lines", request.lines.map(u64::from))] {
        if let Some(value) = value {
            query.push(format!("{key}={value}"));
        }
    }
    if let Some(attempt) = request.attempt {
        query.push(format!("attempt={attempt}"));
    }
    if request.raw {
        query.push("raw=true".to_owned());
    }
    let path = format!("repos/{repo}/actions/jobs/{}/steps/{}/log?{}", request.job, request.step, query.join("&"));
    let window = api.get(&path).await?;
    if out.json {
        println!("{window}");
    } else {
        if window["more_before"] == true {
            eprintln!("(earlier lines left out; this starts at line {})", window["first_line"]);
        }
        for line in window["lines"].as_array().into_iter().flatten() {
            println!("{}", line.as_str().unwrap_or(""));
        }
    }
    Ok(if window["truncated"] == true || window["expired"] == true { 9 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incomplete_or_unexplained_failure_is_untrusted() {
        let digest = |termination: Value| json!({ "jobs": [{ "termination": termination }] });
        assert!(!untrusted(&digest(json!({ "reason": "step_failed", "log_complete": true }))));
        assert!(untrusted(&digest(json!({ "reason": "step_failed", "log_complete": false }))));
        assert!(untrusted(&digest(json!({ "reason": "runner_lost", "log_complete": true }))));
        assert!(untrusted(&digest(json!({ "reason": "unknown", "log_complete": true }))));
        assert_eq!((exit_for(true, false), exit_for(true, true), exit_for(false, true)), (1, 9, 0));
    }
}
