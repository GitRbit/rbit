//! Pull request workflows built from the watch and merge queue endpoints.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::Instant;

use super::{Api, Output, Repo, agent, enqueue, queue_wait, short, text, watch_snapshot};

pub(super) fn deadline(timeout: u64) -> anyhow::Result<Instant> {
    Instant::now().checked_add(Duration::from_secs(timeout)).ok_or_else(|| agent::outcome("usage", "the timeout is too long", None))
}

/// Reasons the watch can stop and the corresponding client exit codes.
#[derive(Clone, Copy)]
enum Outcome {
    Ready,
    Merged,
    Closed,
    ChecksFailed,
    ReviewChanged,
    HeadChanged,
    BaseChanged,
    Blocked,
    Queued,
    QueueFailed,
    Timeout,
}

impl Outcome {
    const fn identifier(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::ChecksFailed => "checks_failed",
            Self::ReviewChanged => "review_changed",
            Self::HeadChanged => "head_changed",
            Self::BaseChanged => "base_changed",
            Self::Blocked => "blocked",
            Self::Queued => "queued",
            Self::QueueFailed => "queue_failed",
            Self::Timeout => "timeout",
        }
    }

    const fn exit_code(self) -> i32 {
        match self {
            Self::Ready | Self::Merged => 0,
            Self::Closed | Self::ChecksFailed | Self::ReviewChanged | Self::Blocked | Self::QueueFailed => 1,
            Self::HeadChanged | Self::BaseChanged => 5,
            Self::Queued | Self::Timeout => 8,
        }
    }
}

fn review_changed(previous: &Value, current: &Value) -> bool {
    ["reviews", "review_count", "threads", "thread_count", "requested_reviewers", "discussion_count"]
        .iter()
        .any(|field| previous[*field] != current[*field])
}

fn active_queue(queue: &Value) -> bool {
    matches!(text(queue, "state"), "waiting" | "testing")
}

fn assess(initial: &Value, snapshot: &Value) -> Option<Outcome> {
    let state = text(snapshot, "state");
    if state == "merged" {
        return Some(Outcome::Merged);
    }
    if state == "closed" {
        return Some(Outcome::Closed);
    }
    if ["head", "head_branch"].iter().any(|field| initial[*field] != snapshot[*field]) {
        return Some(Outcome::HeadChanged);
    }
    if ["base", "base_sha"].iter().any(|field| initial[*field] != snapshot[*field]) {
        return Some(Outcome::BaseChanged);
    }

    let queue = &snapshot["queue"];
    let readiness = &snapshot["readiness"];
    let queue_is_current = queue["head"] == snapshot["head"] && queue["base"] == snapshot["base"];
    if active_queue(queue) {
        return Some(if queue_is_current { Outcome::Queued } else { Outcome::Blocked });
    }
    if queue_is_current && queue["state"] == "failed" {
        return Some(Outcome::QueueFailed);
    }
    if readiness["checks"].as_array().into_iter().flatten().any(|check| check["required"] == true && check["state"] == "failed") {
        return Some(Outcome::ChecksFailed);
    }
    if review_changed(initial, snapshot) {
        return Some(Outcome::ReviewChanged);
    }
    if snapshot["thread_count"] != 0 || snapshot["readiness_checks_protection"] != true || snapshot["base_sha"].is_null() {
        return Some(Outcome::Blocked);
    }
    if readiness["ready"] == true {
        return Some(Outcome::Ready);
    }
    let hard_block = readiness["requirements"].as_array().into_iter().flatten().any(|requirement| {
        requirement["met"] == false
            && match text(requirement, "name") {
                "approvals" | "checks" | "required_checks" => false,
                "no_conflicts" => snapshot["mergeability"] != "unknown",
                _ => true,
            }
    });
    hard_block.then_some(Outcome::Blocked)
}

fn report_handoff(out: &Output, mut snapshot: Value, outcome: Outcome) -> i32 {
    if out.json {
        snapshot["schema"] = json!("rbit.pr-babysit/1");
        snapshot["outcome"] = json!(outcome.identifier());
        println!("{snapshot}");
    } else {
        println!("#{} at {}: {}", snapshot["number"], short(text(&snapshot, "head")), outcome.identifier());
        if snapshot["readiness_checks_protection"] != true {
            println!("  Use a token with repository write access to evaluate branch protection.");
        }
        for requirement in snapshot["readiness"]["requirements"].as_array().into_iter().flatten() {
            if requirement["met"] == false {
                println!("  {}", text(requirement, "says"));
            }
        }
        println!("  Unresolved review threads: {}", snapshot["thread_count"]);
        if let Some(reason) = snapshot["queue"]["reason"].as_str() {
            println!("  Merge queue: {reason}");
        }
    }
    outcome.exit_code()
}

pub(super) async fn babysit(api: &Api, repo: &Repo, number: u64, timeout: u64, out: &Output) -> anyhow::Result<i32> {
    let deadline = deadline(timeout)?;
    let initial = watch_snapshot(api, repo, number, None, 0).await?;
    let mut latest = initial.clone();
    loop {
        if let Some(outcome) = assess(&initial, &latest) {
            return Ok(report_handoff(out, latest, outcome));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(report_handoff(out, latest, Outcome::Timeout));
        }
        // A server wait is integral seconds. Round up instead of repeatedly making
        // immediate requests during the final fraction.
        let seconds = remaining.as_secs().saturating_add(u64::from(remaining.subsec_nanos() != 0)).min(25);
        let cursor = latest["cursor"].as_str().ok_or_else(|| agent::outcome("indeterminate", "the watcher returned no cursor", None))?;
        latest = watch_snapshot(api, repo, number, Some(cursor), seconds).await?;
    }
}

/// Validate the requested head and the observed handoff before asking the existing
/// merge queue to build and test its candidate.
pub(super) async fn full_send(
    api: &Api,
    repo: &Repo,
    number: u64,
    head: &str,
    method: &str,
    timeout: u64,
    out: &Output,
) -> anyhow::Result<i32> {
    deadline(timeout)?;
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(agent::outcome("usage", "pass the full reviewed commit id as --head", None));
    }
    let head = head.to_ascii_lowercase();
    let snapshot = watch_snapshot(api, repo, number, None, 0).await?;
    if snapshot["can_request_merge"] != true || snapshot["readiness_checks_protection"] != true {
        return Err(agent::outcome("forbidden", "a token with repository write access and the merge right is required", None));
    }
    let base = text(&snapshot, "base");
    let queue = &snapshot["queue"];
    if snapshot["state"] == "merged" && queue["state"] == "landed" && queue_matches(queue, &head, base, method) {
        return queue_wait(api, repo, number, queue.clone(), Some(timeout), out).await;
    }
    if snapshot["head"] != head {
        return Err(agent::outcome("head_changed", format!("pull request #{number} is at {}, expected {head}", snapshot["head"]), None));
    }
    if active_queue(queue) {
        if !queue_matches(queue, &head, base, method) {
            return Err(agent::outcome(
                "conflict",
                format!("pull request #{number} already has a different active merge queue entry"),
                None,
            ));
        }
        return queue_wait(api, repo, number, queue.clone(), Some(timeout), out).await;
    }
    if snapshot["state"] != "open"
        || snapshot["base_sha"].is_null()
        || snapshot["readiness"]["ready"] != true
        || snapshot["thread_count"] != 0
    {
        return Err(agent::outcome(
            "blocked",
            format!("pull request #{number} needs attention before queuing ({} unresolved review threads)", snapshot["thread_count"]),
            Some(format!("rbit pr watch {number} --json")),
        ));
    }

    let queued = enqueue(api, repo, number, Some(&head), Some(base), method).await?;
    if !queue_matches(&queued, &head, base, method) {
        return Err(agent::outcome("conflict", "the merge queue returned a different selection", None));
    }
    queue_wait(api, repo, number, queued, Some(timeout), out).await
}

fn queue_matches(place: &Value, head: &str, base: &str, method: &str) -> bool {
    place["head"] == head && place["base"] == base && place["merge_method"] == method
}
