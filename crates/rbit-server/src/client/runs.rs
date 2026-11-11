//! `rbit run`: runs found by commit, never by "the newest run of the repository".
//!
//! A command given no run id works on the runs of one commit: `--sha`, or the current
//! checkout's `HEAD`. When that commit has no run yet the answer is `pending` (exit 8),
//! never another commit's run; `watch` waits for the first run to appear.

use std::{collections::BTreeMap, time::Duration};

use anyhow::{anyhow, bail};
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::{Output, RunCommand},
    util::{clip, encode, git, short, state, text},
};

fn actions(repo: &Repo, rest: &str) -> String {
    format!("repos/{repo}/actions/{rest}")
}

/// A run as programs see it: small and stable.
fn compact(run: &Value) -> Value {
    json!({
        "id": run["id"], "number": run["run_number"], "attempt": run["run_attempt"], "workflow": run["name"],
        "status": state(run), "event": run["event"], "branch": run["head_branch"], "sha": run["head_sha"],
    })
}

/// The commit a command without a run id is about: `sha`, or `HEAD` of the checkout.
fn commit(sha: Option<String>) -> anyhow::Result<String> {
    match sha {
        Some(sha) => Ok(sha),
        None => git(&["rev-parse", "HEAD"])
            .ok_or_else(|| agent::outcome("usage", "no run id and no commit: pass a run id or --sha, or run inside a clone", None)),
    }
}

/// The runs of commit `sha`, newest first; empty when none has started.
async fn runs_of(api: &Api, repo: &Repo, sha: &str) -> anyhow::Result<Vec<Value>> {
    let listed = api.get(&actions(repo, &format!("runs?head_sha={}&per_page=50", encode(sha)))).await?;
    Ok(listed["workflow_runs"].as_array().cloned().unwrap_or_default())
}

/// Run `run`, or every run of the commit; `pending` when the commit has none yet.
async fn resolve(api: &Api, repo: &Repo, run: Option<i64>, sha: Option<String>) -> anyhow::Result<Vec<Value>> {
    if let Some(run) = run {
        return Ok(vec![api.get(&actions(repo, &format!("runs/{run}"))).await?]);
    }
    let sha = commit(sha)?;
    let runs = runs_of(api, repo, &sha).await?;
    if runs.is_empty() {
        let message = format!("no run for {} yet: it has not started, or no workflow applies to it", short(&sha));
        return Err(agent::outcome("pending", message, Some(format!("rbit run watch --sha {sha}"))));
    }
    Ok(runs)
}

/// The file name of `workflow` (a file name, a path or a workflow's `name:`).
async fn workflow_file(api: &Api, repo: &Repo, workflow: &str) -> anyhow::Result<String> {
    let extension = std::path::Path::new(workflow).extension();
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")) {
        return Ok(workflow.rsplit('/').next().unwrap_or(workflow).to_owned());
    }
    let listed = api.get(&actions(repo, "workflows")).await?;
    let found = listed["workflows"].as_array().into_iter().flatten().find(|candidate| text(candidate, "name") == workflow);
    let path = found.map(|found| text(found, "path").to_owned()).ok_or_else(|| anyhow!("no workflow `{workflow}`"))?;
    Ok(path.rsplit('/').next().unwrap_or(&path).to_owned())
}

pub async fn runs(api: &Api, repo: &Repo, command: RunCommand, out: &Output) -> anyhow::Result<i32> {
    match command {
        RunCommand::List { workflow, branch, event, status, sha, limit } => {
            let mut query = vec![format!("per_page={}", limit.clamp(1, 100))];
            for (key, value) in [("branch", &branch), ("event", &event), ("status", &status), ("head_sha", &sha)] {
                if let Some(value) = value {
                    query.push(format!("{key}={}", encode(value)));
                }
            }
            let base = match &workflow {
                Some(workflow) => format!("workflows/{}/runs", encode(&workflow_file(api, repo, workflow).await?)),
                None => "runs".to_owned(),
            };
            let listed = api.get(&actions(repo, &format!("{base}?{}", query.join("&")))).await?;
            let items: Vec<Value> = listed["workflow_runs"].as_array().cloned().unwrap_or_default();
            if out.json {
                let total = listed["total_count"].as_u64().unwrap_or(items.len() as u64);
                let rows = agent::select(items.iter().map(compact).collect(), &out.fields);
                let mut body = json!({ "runs": rows, "total": total });
                if total > items.len() as u64 {
                    body["truncated"] = json!(true);
                }
                agent::print("rbit.runs/1", body);
                return Ok(0);
            }
            for run in &items {
                println!(
                    "{:<12} {:<20} {:<40} {:<20} {:<12} {}",
                    state(run),
                    clip(text(run, "name"), 20),
                    clip(text(run, "display_title"), 40),
                    clip(text(run, "head_branch"), 20),
                    short(text(run, "head_sha")),
                    run["id"]
                );
            }
        }
        RunCommand::View { run, sha, log, log_failed, job, exit_status } => {
            let found = resolve(api, repo, run, sha).await?;
            return super::runs_view::view(api, repo, &found, &super::runs_view::Show { log, log_failed, job, exit_status }, out).await;
        }
        RunCommand::Watch { run, sha, exit_status, interval, timeout } => {
            return watch(api, repo, run, sha, Wait { exit_status, interval, timeout }, out).await;
        }
        RunCommand::Rerun { run, failed, job } => {
            let path = match (job, failed) {
                (Some(job), _) => actions(repo, &format!("jobs/{job}/rerun")),
                (None, true) => actions(repo, &format!("runs/{run}/rerun-failed-jobs")),
                (None, false) => actions(repo, &format!("runs/{run}/rerun")),
            };
            api.json(Method::POST, &path, Some(&json!({}))).await?;
            report(out, "rbit.rerun/1", json!({ "run": run, "job": job, "failed_only": failed }), &format!("Run {run} runs again"));
        }
        RunCommand::Cancel { run } => {
            api.json(Method::POST, &actions(repo, &format!("runs/{run}/cancel")), Some(&json!({}))).await?;
            report(out, "rbit.cancel/1", json!({ "run": run }), &format!("Run {run} was cancelled"));
        }
        RunCommand::Download { run, sha, name, pattern, dir } => {
            let found = resolve(api, repo, run, sha).await?;
            let [one] = found.as_slice() else {
                let ids: Vec<String> = found.iter().map(|run| run["id"].to_string()).collect();
                return Err(agent::outcome(
                    "usage",
                    format!("the commit has {} runs ({}): pass one run id", ids.len(), ids.join(", ")),
                    None,
                ));
            };
            let id = one["id"].as_i64().ok_or_else(|| anyhow!("the run has no id"))?;
            super::commands::download_run(api, repo, id, &name, pattern.as_deref(), &dir).await?;
        }
    }
    Ok(0)
}

/// A short confirmation: one JSON object with `--json`, a line on stderr otherwise.
pub fn report(out: &Output, schema: &str, body: Value, line: &str) {
    if out.json {
        agent::print(schema, body);
    } else {
        eprintln!("{line}");
    }
}

/// How `run watch` waits.
struct Wait {
    exit_status: bool,
    interval: u64,
    timeout: u64,
}

/// `run watch`: each job's state as it changes, until every run (`run`, or every run of the
/// commit, waiting for the first to appear) completes or `timeout` seconds pass.
async fn watch(api: &Api, repo: &Repo, run: Option<i64>, sha: Option<String>, wait: Wait, out: &Output) -> anyhow::Result<i32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait.timeout);
    let sha = if run.is_some() { None } else { Some(commit(sha)?) };
    let mut seen: BTreeMap<i64, String> = BTreeMap::new();
    loop {
        let runs = match (run, &sha) {
            (Some(id), _) => vec![api.get(&actions(repo, &format!("runs/{id}"))).await?],
            (None, Some(sha)) => runs_of(api, repo, sha).await?,
            (None, None) => bail!("nothing to watch"),
        };
        for run in &runs {
            let id = run["id"].as_i64().unwrap_or_default();
            let jobs = api.get(&actions(repo, &format!("runs/{id}/jobs?per_page=100"))).await?;
            for job in jobs["jobs"].as_array().into_iter().flatten() {
                let (key, now) = (job["id"].as_i64().unwrap_or_default(), state(job));
                if !out.json && seen.get(&key) != Some(&now) {
                    println!("{:<12} {} / {}", now, text(run, "name"), text(job, "name"));
                }
                seen.insert(key, now);
            }
        }
        let done = !runs.is_empty() && runs.iter().all(|run| run["status"].as_str() == Some("completed"));
        if done {
            let green = runs.iter().all(|run| run["conclusion"].as_str() == Some("success"));
            if out.json {
                let state = if green { "success" } else { "failure" };
                agent::print("rbit.watch/1", json!({ "sha": sha, "state": state, "runs": runs.iter().map(compact).collect::<Vec<_>>() }));
            } else {
                for run in &runs {
                    println!("Run #{} {}: {}", run["run_number"], text(run, "name"), state(run));
                }
            }
            return Ok(i32::from(wait.exit_status && !green));
        }
        if tokio::time::Instant::now() >= deadline {
            let what =
                sha.as_deref().map_or_else(|| format!("run {}", run.unwrap_or_default()), |sha| format!("the runs of {}", short(sha)));
            let message = if runs.is_empty() { format!("no run appeared for {what}") } else { format!("{what} did not finish in time") };
            return Err(agent::outcome("timeout", message, None));
        }
        tokio::time::sleep(Duration::from_secs(wait.interval.max(1))).await;
    }
}
