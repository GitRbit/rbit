//! `rbit run view`: runs, their jobs and the steps that did not succeed, and the logs
//! asked for.

use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::Output,
    util::{short, state, text},
};

/// What `run view` shows besides the summary.
pub struct Show {
    pub log: bool,
    pub log_failed: bool,
    pub job: Option<i64>,
    pub exit_status: bool,
}

/// Whether a job or run ended without succeeding: failed, timed out or cancelled.
fn unsuccessful(value: &Value) -> bool {
    value["status"].as_str() == Some("completed") && !matches!(value["conclusion"].as_str(), Some("success" | "skipped"))
}

pub async fn view(api: &Api, repo: &Repo, runs: &[Value], show: &Show, out: &Output) -> anyhow::Result<i32> {
    let mut shown = Vec::new();
    for run in runs {
        let id = run["id"].as_i64().unwrap_or_default();
        let listed = api.get(&format!("repos/{repo}/actions/runs/{id}/jobs?per_page=100")).await?;
        let jobs: Vec<Value> = listed["jobs"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|job| show.job.is_none_or(|wanted| job["id"].as_i64() == Some(wanted)))
            .collect();
        if out.json {
            let jobs: Vec<Value> = jobs.iter().map(job_summary).collect();
            shown.push(json!({ "id": run["id"], "number": run["run_number"], "attempt": run["run_attempt"], "workflow": run["name"],
                "status": state(run), "sha": run["head_sha"], "branch": run["head_branch"], "url": run["html_url"], "jobs": jobs }));
        } else {
            print_run(run, &jobs);
        }
        if show.log || show.log_failed {
            for job in jobs.iter().filter(|job| show.log || unsuccessful(job)) {
                let bytes = api.download(&format!("repos/{repo}/actions/jobs/{}/logs", job["id"])).await?;
                // With `--json`, stdout holds only the JSON object.
                for line in String::from_utf8_lossy(&bytes).lines() {
                    if out.json {
                        eprintln!("{}\t{line}", text(job, "name"));
                    } else {
                        println!("{}\t{line}", text(job, "name"));
                    }
                }
            }
        }
    }
    if out.json {
        agent::print("rbit.run/1", json!({ "runs": shown }));
    }
    if show.exit_status {
        if runs.iter().any(|run| run["status"].as_str() != Some("completed")) {
            return Ok(8);
        }
        if runs.iter().any(|run| run["conclusion"].as_str() != Some("success")) {
            return Ok(1);
        }
    }
    Ok(0)
}

/// A job for programs: its state and the steps that did not succeed.
fn job_summary(job: &Value) -> Value {
    let steps: Vec<Value> = job["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|step| step["conclusion"].as_str() != Some("success"))
        .map(|step| json!({ "number": step["number"], "name": step["name"], "status": state(step) }))
        .collect();
    json!({ "id": job["id"], "name": job["name"], "status": state(job), "steps": steps })
}

fn print_run(run: &Value, jobs: &[Value]) {
    println!("{} #{} {} ({})", text(run, "name"), run["run_number"], text(run, "display_title"), state(run));
    println!("{} on {} at {}", text(run, "event"), text(run, "head_branch"), short(text(run, "head_sha")));
    println!("{}", text(run, "html_url"));
    for job in jobs {
        println!("  {:<12} {} ({})", state(job), text(job, "name"), job["id"]);
        for step in job["steps"].as_array().into_iter().flatten() {
            if step["conclusion"].as_str() != Some("success") {
                println!("      {:<10} {}", state(step), text(step, "name"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_and_timed_out_jobs_count_as_unsuccessful() {
        for conclusion in ["failure", "cancelled", "timed_out"] {
            assert!(unsuccessful(&json!({ "status": "completed", "conclusion": conclusion })), "{conclusion}");
        }
        assert!(!unsuccessful(&json!({ "status": "completed", "conclusion": "success" })));
        assert!(!unsuccessful(&json!({ "status": "completed", "conclusion": "skipped" })));
        assert!(!unsuccessful(&json!({ "status": "in_progress", "conclusion": null })));
    }

    #[test]
    fn a_job_summary_keeps_only_the_steps_that_did_not_succeed() {
        let job = json!({ "id": 7, "name": "test", "status": "completed", "conclusion": "failure", "steps": [
            { "number": 1, "name": "checkout", "status": "completed", "conclusion": "success" },
            { "number": 2, "name": "tests", "status": "completed", "conclusion": "failure" },
        ]});
        assert_eq!(job_summary(&job)["steps"], json!([{ "number": 2, "name": "tests", "status": "failure" }]));
    }
}
