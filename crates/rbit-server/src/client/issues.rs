//! `rbit issue`: issues as an agent's task queue, each command with `--json`.
//!
//! * `list` gives one short row per issue (pull requests left out), `--mine` those assigned
//!   to the caller.
//! * `view` gives the title, the body and the last few comments, bounded.
//! * `create`, `comment`, `close` and `label` change one issue; a body comes from `--body`,
//!   `--body-file`, or `-` for standard input.

use std::io::Read as _;

use anyhow::Context as _;
use clap::Subcommand;
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Api, Repo, agent,
    commands::Output,
    runs::report,
    util::{clip, encode, text},
};

/// Comments `issue view` shows, the newest ones.
const VIEW_COMMENTS: usize = 3;
/// Longest body or comment `issue view --json` carries, in characters.
const VIEW_TEXT_CHARS: usize = 4000;

#[derive(Debug, Subcommand)]
pub enum IssueCommand {
    /// Issues, newest first: one short row each.
    List {
        #[arg(short, long, default_value = "open")]
        state: String,
        /// Only issues with this label (repeatable).
        #[arg(short, long)]
        label: Vec<String>,
        /// Only issues assigned to you.
        #[arg(long)]
        mine: bool,
        #[arg(short = 'L', long, default_value_t = 20)]
        limit: u32,
    },
    /// An issue: title, body and its last few comments.
    View { number: u64 },
    /// Open an issue.
    Create {
        #[arg(short, long)]
        title: String,
        #[arg(short, long)]
        body: Option<String>,
        /// Read the body from a file, `-` for standard input.
        #[arg(long, conflicts_with = "body")]
        body_file: Option<std::path::PathBuf>,
        #[arg(short, long)]
        label: Vec<String>,
    },
    /// Comment on an issue.
    Comment {
        number: u64,
        #[arg(short, long)]
        body: Option<String>,
        #[arg(long, conflicts_with = "body")]
        body_file: Option<std::path::PathBuf>,
    },
    /// Close an issue (`--reason not_planned` for one that will not be done).
    /// Take an issue: assign it to yourself only if nobody holds it (exit 5, `conflict`,
    /// naming the holder, otherwise).
    Take { number: u64 },
    /// Issues and pull requests whose title or body has every word, best first.
    Search {
        #[arg(required = true)]
        words: Vec<String>,
        #[arg(short, long, default_value = "all")]
        state: String,
    },
    Close {
        number: u64,
        #[arg(long, default_value = "completed")]
        reason: String,
    },
    /// Add labels to an issue, or remove them with `--remove`.
    Label {
        number: u64,
        #[arg(required = true)]
        labels: Vec<String>,
        #[arg(long)]
        remove: bool,
    },
}

/// The text of `--body`, of `--body-file` (`-`: standard input), or empty.
pub(super) fn body(body: Option<String>, file: Option<std::path::PathBuf>) -> anyhow::Result<String> {
    match (body, file) {
        (Some(body), _) => Ok(body),
        (None, Some(file)) if file.as_os_str() == "-" => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).context("cannot read the body from standard input")?;
            Ok(text)
        }
        (None, Some(file)) => std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display())),
        (None, None) => Ok(String::new()),
    }
}

/// An issue as a short row.
fn row(issue: &Value) -> Value {
    let labels: Vec<&str> = issue["labels"].as_array().into_iter().flatten().filter_map(|label| label["name"].as_str()).collect();
    json!({
        "number": issue["number"], "title": issue["title"], "state": issue["state"], "labels": labels,
        "assignee": issue["assignee"]["login"], "updated": issue["updated_at"],
    })
}

/// `text` cut to `chars` characters, with whether it was cut.
fn bounded(text: &str, chars: usize) -> (String, bool) {
    if text.chars().count() <= chars { (text.to_owned(), false) } else { (text.chars().take(chars).collect(), true) }
}

pub async fn issues(api: &Api, repo: &Repo, command: IssueCommand, out: &Output) -> anyhow::Result<i32> {
    match command {
        IssueCommand::List { state, label, mine, limit } => {
            let mut query = format!("state={}&per_page={}", encode(&state), limit.clamp(1, 100));
            if !label.is_empty() {
                query = format!("{query}&labels={}", encode(&label.join(",")));
            }
            if mine {
                let me = api.get("user").await?;
                query = format!("{query}&assignee={}", encode(text(&me, "login")));
            }
            let listed = api.get(&format!("repos/{repo}/issues?{query}")).await?;
            let issues: Vec<&Value> = listed.as_array().into_iter().flatten().filter(|issue| issue.get("pull_request").is_none()).collect();
            if out.json {
                let rows = agent::select(issues.iter().map(|issue| row(issue)).collect(), &out.fields);
                agent::print("rbit.issues/1", json!({ "issues": rows }));
                return Ok(0);
            }
            for issue in issues {
                let labels = row(issue)["labels"]
                    .as_array()
                    .map(|labels| labels.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","))
                    .unwrap_or_default();
                println!("#{:<6} {:<60} {labels}", issue["number"], clip(text(issue, "title"), 60));
            }
        }
        IssueCommand::View { number } => {
            let issue = api.get(&format!("repos/{repo}/issues/{number}")).await?;
            let comments = api.get(&format!("repos/{repo}/issues/{number}/comments?per_page=100")).await?;
            let all = comments.as_array().cloned().unwrap_or_default();
            let recent = &all[all.len().saturating_sub(VIEW_COMMENTS)..];
            if out.json {
                let (body, cut) = bounded(text(&issue, "body"), VIEW_TEXT_CHARS);
                let recent: Vec<Value> = recent
                    .iter()
                    .map(|comment| json!({ "author": comment["user"]["login"], "at": comment["created_at"], "body": bounded(text(comment, "body"), VIEW_TEXT_CHARS).0 }))
                    .collect();
                let mut value = row(&issue);
                value["body"] = json!(body);
                value["body_truncated"] = json!(cut);
                value["comments"] = json!({ "total": all.len(), "recent": recent });
                value["url"] = issue["html_url"].clone();
                agent::print("rbit.issue/1", value);
            } else {
                println!("#{number} {} ({})", text(&issue, "title"), text(&issue, "state"));
                println!("{}", text(&issue, "html_url"));
                if !text(&issue, "body").is_empty() {
                    println!("\n{}", text(&issue, "body"));
                }
                for comment in recent {
                    println!("\n--- {} at {}\n{}", text(&comment["user"], "login"), text(comment, "created_at"), text(comment, "body"));
                }
            }
        }
        IssueCommand::Create { title, body: text_body, body_file, label } => {
            let request = json!({ "title": title, "body": body(text_body, body_file)?, "labels": label });
            let created = api.json(Method::POST, &format!("repos/{repo}/issues"), Some(&request)).await?;
            if out.json {
                agent::print("rbit.issue/1", row(&created));
            } else {
                println!("{}", text(&created, "html_url"));
            }
        }
        IssueCommand::Comment { number, body: text_body, body_file } => {
            let request = json!({ "body": body(text_body, body_file)? });
            let comment = api.json(Method::POST, &format!("repos/{repo}/issues/{number}/comments"), Some(&request)).await?;
            report(out, "rbit.comment/1", json!({ "issue": number, "id": comment["id"] }), &format!("Commented on #{number}"));
        }
        IssueCommand::Take { number } => {
            let taken = api.json(Method::POST, &format!("repos/{repo}/issues/{number}/take"), Some(&json!({}))).await?;
            report(out, "rbit.issue-taken/1", json!({ "issue": number, "title": taken["title"] }), &format!("#{number} is yours"));
        }
        IssueCommand::Search { words, state } => search(api, repo, &words, &state, out).await?,
        IssueCommand::Close { number, reason } => {
            let request = json!({ "state": "closed", "state_reason": reason });
            api.json(Method::PATCH, &format!("repos/{repo}/issues/{number}"), Some(&request)).await?;
            report(out, "rbit.issue-closed/1", json!({ "issue": number, "reason": reason }), &format!("Closed #{number}"));
        }
        IssueCommand::Label { number, labels, remove } => {
            if remove {
                for label in &labels {
                    api.json(Method::DELETE, &format!("repos/{repo}/issues/{number}/labels/{}", encode(label)), None).await?;
                }
            } else {
                api.json(Method::POST, &format!("repos/{repo}/issues/{number}/labels"), Some(&json!({ "labels": labels }))).await?;
            }
            let line = format!("{} {} on #{number}", if remove { "Removed" } else { "Added" }, labels.join(", "));
            report(out, "rbit.labels/1", json!({ "issue": number, "labels": labels, "removed": remove }), &line);
        }
    }
    Ok(0)
}

/// `issue search`: the issues whose title or body has every word.
async fn search(api: &Api, repo: &Repo, words: &[String], state: &str, out: &Output) -> anyhow::Result<()> {
    let query = format!("q={}&state={}", encode(&words.join(" ")), encode(state));
    let mut found = api.get(&format!("repos/{repo}/agent/issues?{query}")).await?;
    if out.json {
        if let Some(object) = found.as_object_mut() {
            object.remove("schema");
        }
        agent::print("rbit.issue-search/1", found);
        return Ok(());
    }
    for item in found["items"].as_array().into_iter().flatten() {
        let kind = if item["pull_request"] == true { "PR" } else { "issue" };
        println!("#{:<6} {:<6} {:<7} {}", item["number"], text(item, "state"), kind, clip(text(item, "title"), 80));
    }
    println!("{} found", found["total"]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_short_and_bodies_bounded() {
        let issue = json!({ "number": 7, "title": "Fix it", "state": "open", "labels": [{ "name": "bug" }], "assignee": { "login": "ada" }, "updated_at": "x", "body": "long" });
        assert_eq!(
            row(&issue),
            json!({ "number": 7, "title": "Fix it", "state": "open", "labels": ["bug"], "assignee": "ada", "updated": "x" })
        );
        assert_eq!(bounded("abcdef", 3), ("abc".to_owned(), true));
        assert_eq!(bounded("ab", 3), ("ab".to_owned(), false));
        assert_eq!(body(Some("given".to_owned()), None).unwrap(), "given");
        assert_eq!(body(None, None).unwrap(), "");
    }
}
