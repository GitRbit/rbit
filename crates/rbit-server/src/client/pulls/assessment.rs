//! Read-only evidence for refreshing a PR or continuing someone else's work.

use serde_json::{Value, json};

use super::{Api, Output, Repo, agent, compact, encode, short, text};

pub(super) enum Mode {
    Modernize,
    Takeover,
}

pub(super) async fn show(api: &Api, repo: &Repo, number: u64, mode: Mode, out: &Output) -> anyhow::Result<()> {
    let path = format!("repos/{repo}/pulls/{number}");
    let watch_path = format!("{path}/watch?timeout=0");
    let (pull, watch) = tokio::try_join!(api.get(&path), api.get(&watch_path))?;
    same_revision(&watch, &pull)?;

    // A merged PR's diff is against its recorded base, even after the branch advances.
    let base = pull["base"]["sha"].as_str();
    let (comparison, manifest) = if let Some(base) = base {
        let head = text(&watch, "head");
        let compare_path = format!("repos/{repo}/agent/compare?base={}&head={}", encode(base), encode(head));
        let manifest_path = format!("{path}/manifest");
        let (comparison, manifest) = tokio::try_join!(api.get(&compare_path), api.get(&manifest_path))?;
        if manifest["head"] != head || manifest["base"] != base {
            return Err(moved());
        }
        (comparison, manifest)
    } else {
        (Value::Null, Value::Null)
    };

    let mut details = compact(&pull);
    details["author"] = pull["user"]["login"].clone();
    details["body"] = pull["body"].clone();
    details["base_sha"] = pull["base"]["sha"].clone();
    details["head_repository"] = pull["head"]["repo"]["full_name"].clone();
    let disposition = match (text(&watch, "state"), base, comparison["ahead"].as_u64()) {
        ("merged", _, _) => "merged",
        ("closed", _, _) => "closed",
        (_, None, _) => "base_missing",
        (_, _, Some(0)) => "head_already_in_base",
        _ => "needs_assessment",
    };
    let schema = match mode {
        Mode::Modernize => "rbit.pr-modernize/1",
        Mode::Takeover => "rbit.pr-takeover/1",
    };
    if out.json {
        agent::print(
            schema,
            json!({ "pull": details, "comparison": comparison, "manifest": manifest, "watch": watch, "disposition": disposition }),
        );
    } else {
        println!("#{number} {} ({})", text(&details, "title"), text(&watch, "state"));
        println!("  Head: {} at {}", text(&watch, "head_branch"), short(text(&watch, "head")));
        println!("  Base: {} at {}", text(&watch, "base"), base.map_or("(missing)", short));
        if !comparison.is_null() {
            println!("  Commits: {} ahead, {} behind", comparison["ahead"], comparison["behind"]);
        }
        println!("  Unresolved review threads: {}", watch["thread_count"]);
        println!("  Assessment: {}", disposition.replace('_', " "));
        println!("{}", text(&details, "url"));
    }
    Ok(())
}

fn same_revision(watch: &Value, pull: &Value) -> anyhow::Result<()> {
    if watch["head"] != pull["head"]["sha"]
        || watch["base"] != pull["base"]["ref"]
        || watch["state"] != compact(pull)["state"]
        || (watch["state"] != "merged" && watch["base_sha"] != pull["base"]["sha"])
    {
        return Err(moved());
    }
    Ok(())
}

fn moved() -> anyhow::Error {
    agent::outcome("conflict", "the pull request changed while collecting evidence", Some("run the command again".to_owned()))
}
