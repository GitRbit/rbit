//! `rbit pr`: create, list, view, merge and checks, each with `--json`.
//!
//! * `pr merge` always names the head it expects: `--head`, or the head it reads just
//!   before merging, so a push that lands in between is refused (`head_changed`, exit 5)
//!   instead of merged untested.
//! * `pr checks` reads "nothing reported yet" as pending (exit 8), never as green.

use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::{Output, PrCommand},
    runs::report,
    util::{clip, encode, git, short, text},
};

/// A pull request for programs.
fn compact(pull: &Value) -> Value {
    json!({
        "number": pull["number"], "title": pull["title"], "state": if pull["merged"].as_bool() == Some(true) { json!("merged") } else { pull["state"].clone() },
        "draft": pull["draft"], "head": pull["head"]["ref"], "head_sha": pull["head"]["sha"], "base": pull["base"]["ref"], "url": pull["html_url"],
    })
}

/// The current Git branch.
fn current_branch() -> Option<String> {
    git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|branch| branch != "HEAD")
}

pub async fn pulls(api: &Api, repo: &Repo, command: PrCommand, out: &Output) -> anyhow::Result<i32> {
    match command {
        PrCommand::Create { title, body, base, head, draft } => {
            let head = head.or_else(current_branch).ok_or_else(|| agent::outcome("usage", "which branch? pass --head", None))?;
            let base = match base {
                Some(base) => base,
                None => api.get(&format!("repos/{repo}")).await?["default_branch"].as_str().unwrap_or("main").to_owned(),
            };
            let request = json!({ "title": title, "body": body, "base": base, "head": head, "draft": draft });
            let created = api.json(Method::POST, &format!("repos/{repo}/pulls"), Some(&request)).await?;
            if out.json {
                agent::print("rbit.pr/1", compact(&created));
            } else {
                println!("{}", text(&created, "html_url"));
            }
        }
        PrCommand::Ensure { title, body, base, head, ready, request_id } => {
            ensure(api, repo, (title, body, base, head), (ready, request_id), out).await?;
        }
        PrCommand::Queue { number, head, squash, rebase, watch, timeout } => {
            let method = if squash {
                "squash"
            } else if rebase {
                "rebase"
            } else {
                "merge"
            };
            let request = json!({ "head": head, "merge_method": method });
            let place = api.json(Method::POST, &format!("repos/{repo}/pulls/{number}/queue"), Some(&request)).await?;
            return queue_wait(api, repo, number, place, (watch, timeout), out).await;
        }
        PrCommand::UpdateBranch { number, head } => {
            write(api, repo, &format!("pulls/{number}/update-branch"), json!({ "expected_head": head }), out).await?;
        }
        PrCommand::Revert { commit, base } => write(api, repo, "agent/revert", json!({ "commit": commit, "base": base }), out).await?,
        PrCommand::Dequeue { number } => {
            api.json(Method::DELETE, &format!("repos/{repo}/pulls/{number}/queue"), None).await?;
            report(out, "rbit.dequeued/1", json!({ "number": number }), &format!("#{number} left the merge queue"));
        }
        PrCommand::Ready { number, head, mark, watch, timeout } => {
            return ready(api, repo, number, head, (mark, watch, timeout), out).await;
        }
        PrCommand::List { state, head, limit } => {
            let head = head.map(|head| format!("&head={}", encode(&format!("{}:{head}", repo.owner)))).unwrap_or_default();
            let path = format!("repos/{repo}/pulls?state={}&per_page={}{head}", encode(&state), limit.clamp(1, 100));
            let listed = api.get(&path).await?;
            let pulls = listed.as_array().cloned().unwrap_or_default();
            if out.json {
                agent::print("rbit.prs/1", json!({ "pulls": agent::select(pulls.iter().map(compact).collect(), &out.fields) }));
                return Ok(0);
            }
            for pull in &pulls {
                let (base, head) = (text(&pull["base"], "ref"), text(&pull["head"], "ref"));
                println!("#{:<6} {:<50} {base} <- {head}", pull["number"], clip(text(pull, "title"), 50));
            }
        }
        PrCommand::View { number } => {
            let pull = api.get(&format!("repos/{repo}/pulls/{number}")).await?;
            if out.json {
                let mut body = compact(&pull);
                body["mergeable"] = pull["mergeable"].clone();
                body["body"] = pull["body"].clone();
                agent::print("rbit.pr/1", body);
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
        PrCommand::Watch { number, cursor, timeout } => {
            let mut path = format!("repos/{repo}/pulls/{number}/watch?timeout={timeout}");
            if let Some(cursor) = cursor {
                path.push_str("&cursor=");
                path.push_str(&encode(&cursor));
            }
            let mut body = api.get(&path).await?;
            if out.json {
                if let Some(object) = body.as_object_mut() {
                    object.remove("schema");
                }
                agent::print("rbit.pr-watch/1", body);
            } else {
                let update = if body["changed"] == true { "snapshot" } else { "unchanged" };
                println!("#{number} {} at {} ({update})", text(&body, "state"), short(text(&body, "head")));
                for requirement in body["readiness"]["requirements"].as_array().into_iter().flatten() {
                    if requirement["met"] == false {
                        println!("  {}", text(requirement, "says"));
                    }
                }
                println!("  Unresolved review threads: {}", body["thread_count"]);
                println!("  Cursor: {}", text(&body, "cursor"));
            }
        }
        PrCommand::Merge { number, merge: _, squash, rebase, head } => {
            let method = if squash {
                "squash"
            } else if rebase {
                "rebase"
            } else {
                "merge"
            };
            let head = match head {
                Some(head) => head,
                None => text(&api.get(&format!("repos/{repo}/pulls/{number}")).await?["head"], "sha").to_owned(),
            };
            let request = json!({ "merge_method": method, "sha": head });
            let merged = api.json(Method::PUT, &format!("repos/{repo}/pulls/{number}/merge"), Some(&request)).await?;
            let sha = text(&merged, "sha");
            let line = format!("Merged #{number} ({method}) at {} as {sha}", short(&head));
            report(out, "rbit.merge/1", json!({ "number": number, "method": method, "head": head, "sha": sha }), &line);
        }
        PrCommand::Checks { number, watch, interval } => return checks(api, repo, number, watch, interval, out).await,
    }
    Ok(0)
}

/// The commit statuses of `number`'s head: exit 0 when all passed, 1 when one failed, 8
/// while some are pending or none has been reported.
async fn checks(api: &Api, repo: &Repo, number: u64, watch: bool, interval: u64, out: &Output) -> anyhow::Result<i32> {
    loop {
        let pull = api.get(&format!("repos/{repo}/pulls/{number}")).await?;
        let sha = text(&pull["head"], "sha").to_owned();
        let status = api.get(&format!("repos/{repo}/commits/{sha}/status")).await?;
        let statuses = status["statuses"].as_array().cloned().unwrap_or_default();
        let failed = statuses.iter().any(|status| matches!(text(status, "state"), "failure" | "error"));
        // Nothing reported yet is not green: the checks may not have started.
        let pending = statuses.is_empty() || statuses.iter().any(|status| text(status, "state") == "pending");
        if !watch || failed || !pending {
            let state = if failed {
                "failure"
            } else if pending {
                "pending"
            } else {
                "success"
            };
            if out.json {
                let checks: Vec<Value> = statuses
                    .iter()
                    .map(|status| json!({ "context": status["context"], "state": status["state"], "url": status["target_url"] }))
                    .collect();
                agent::print("rbit.checks/1", json!({ "number": number, "sha": sha, "state": state, "checks": checks }));
            } else {
                for status in &statuses {
                    println!("{:<8} {:<50} {}", text(status, "state"), clip(text(status, "context"), 50), text(status, "target_url"));
                }
                if statuses.is_empty() {
                    println!("pending  nothing reported yet on {}", short(&sha));
                }
            }
            return Ok(match state {
                "failure" => 1,
                "pending" => 8,
                _ => 0,
            });
        }
        tokio::time::sleep(Duration::from_secs(interval.max(1))).await;
    }
}

/// `pr ready`: the readiness of `number` (marked ready first with `mark`), waited on with
/// `watch` until it is ready or a required check failed, at most `timeout` seconds.
async fn ready(
    api: &Api,
    repo: &Repo,
    number: u64,
    head: Option<String>,
    (mark, watch, timeout): (bool, bool, u64),
    out: &Output,
) -> anyhow::Result<i32> {
    let query = head.as_deref().map(|head| format!("?head={}", encode(head))).unwrap_or_default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut answer = if mark {
        api.json(Method::POST, &format!("repos/{repo}/pulls/{number}/ready"), Some(&json!({ "head": head }))).await?
    } else {
        api.get(&format!("repos/{repo}/pulls/{number}/readiness{query}")).await?
    };
    loop {
        let checks = answer["checks"].as_array().cloned().unwrap_or_default();
        let required = |state: &str| checks.iter().any(|check| check["required"] == true && check["state"] == state);
        let code = if answer["ready"] == true {
            0
        } else if required("failed") {
            1
        } else if required("expected") || required("running") {
            8
        } else {
            1
        };
        if code != 8 || !watch || std::time::Instant::now() >= deadline {
            if out.json {
                let mut body = answer;
                if let Some(object) = body.as_object_mut() {
                    object.remove("schema");
                }
                agent::print("rbit.readiness/1", body);
            } else {
                println!("#{number} at {}: {}", short(text(&answer, "head")), if code == 0 { "ready to merge" } else { "not ready" });
                for requirement in answer["requirements"].as_array().into_iter().flatten().filter(|requirement| requirement["met"] == false)
                {
                    println!("  - {}", text(requirement, "says"));
                }
                for check in checks.iter().filter(|check| check["required"] == true) {
                    println!("  {:<8} {}", text(check, "state"), text(check, "context"));
                }
            }
            return Ok(code);
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        answer = api.get(&format!("repos/{repo}/pulls/{number}/readiness{query}")).await?;
    }
}

/// `pr queue`: reports the place in line, and with `watch` waits until the entry left it.
async fn queue_wait(
    api: &Api,
    repo: &Repo,
    number: u64,
    mut place: Value,
    (watch, timeout): (bool, u64),
    out: &Output,
) -> anyhow::Result<i32> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut said = String::new();
    loop {
        let state = text(&place, "state").to_owned();
        let code = match state.as_str() {
            "landed" => Some(0),
            "failed" => Some(1),
            "removed" => Some(5),
            _ => None,
        };
        if !out.json {
            let line = format!(
                "#{number}: {state}, position {}{}",
                place["position"],
                place["reason"].as_str().map(|reason| format!(" ({reason})")).unwrap_or_default()
            );
            if line != said {
                eprintln!("{line}");
                said = line;
            }
        }
        if code.is_some() || !watch || std::time::Instant::now() >= deadline {
            if out.json {
                if let Some(object) = place.as_object_mut() {
                    object.remove("schema");
                }
                agent::print("rbit.queue/1", place);
            }
            return Ok(code.unwrap_or(if watch { 8 } else { 0 }));
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        place = api.get(&format!("repos/{repo}/pulls/{number}/queue")).await?;
    }
}

/// `pr ensure`: the pull request for a branch, created or recovered.
async fn ensure(
    api: &Api,
    repo: &Repo,
    (title, body, base, head): (String, String, Option<String>, Option<String>),
    (ready, request_id): (bool, Option<String>),
    out: &Output,
) -> anyhow::Result<()> {
    let head = head.or_else(current_branch).ok_or_else(|| agent::outcome("usage", "which branch? pass --head", None))?;
    let base = match base {
        Some(base) => base,
        None => api.get(&format!("repos/{repo}")).await?["default_branch"].as_str().unwrap_or("main").to_owned(),
    };
    let request = json!({ "title": title, "body": body, "base": base, "head": head, "draft": !ready, "request_id": request_id });
    let ensured = api.json(Method::POST, &format!("repos/{repo}/pulls/ensure"), Some(&request)).await?;
    if out.json {
        agent::print(
            "rbit.pr-ensure/1",
            json!({ "number": ensured["number"], "outcome": ensured["outcome"], "pull": compact(&ensured["pull"]) }),
        );
    } else {
        println!("{} ({})", text(&ensured["pull"], "html_url"), text(&ensured, "outcome"));
    }
    Ok(())
}

/// A server-side write (`update-branch`, `revert`): its answer, as JSON or one line.
async fn write(api: &Api, repo: &Repo, path: &str, body: Value, out: &Output) -> anyhow::Result<()> {
    let mut answer = api.json(Method::POST, &format!("repos/{repo}/{path}"), Some(&body)).await?;
    let schema = text(&answer, "schema").to_owned();
    if out.json {
        if let Some(object) = answer.as_object_mut() {
            object.remove("schema");
        }
        agent::print(&schema, answer);
    } else if let Some(number) = answer["number"].as_u64().filter(|_| schema == "rbit.revert/1") {
        println!("opened #{number} reverting it");
    } else {
        println!("{}", text(&answer, "head"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merged_pull_request_reads_merged() {
        let pull = json!({ "number": 3, "state": "closed", "merged": true, "draft": false, "head": { "ref": "x", "sha": "abc" }, "base": { "ref": "main" } });
        assert_eq!(compact(&pull)["state"], "merged");
        assert_eq!(compact(&pull)["head_sha"], "abc");
        let open = json!({ "number": 4, "state": "open", "merged": false });
        assert_eq!(compact(&open)["state"], "open");
    }
}
