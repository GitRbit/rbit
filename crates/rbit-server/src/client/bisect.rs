//! `rbit bisect <good> <bad>`: what CI already recorded between two commits, and the one
//! to test next (agent plan step 4.9). Nothing runs; the server reads recorded results.

use serde_json::Value;

use super::{
    Api, Repo, agent,
    commands::Output,
    util::{encode, short, text},
};

pub async fn bisect(api: &Api, repo: &Repo, good: &str, bad: &str, workflow: Option<&str>, out: &Output) -> anyhow::Result<i32> {
    let mut query = format!("good={}&bad={}", encode(good), encode(bad));
    if let Some(workflow) = workflow {
        query = format!("{query}&workflow={}", encode(workflow));
    }
    let mut found: Value = api.get(&format!("repos/{repo}/agent/bisect?{query}")).await?;
    if out.json {
        if let Some(object) = found.as_object_mut() {
            object.remove("schema");
        }
        agent::print("rbit.bisect/1", found);
        return Ok(0);
    }
    println!("{} commits: {} passed, {} failed, {} untested", found["commits"], found["passed"], found["failed"], found["untested"]);
    println!("  last good {}, first bad {}", short(text(&found, "last_good")), short(text(&found, "first_bad")));
    match (found["culprit"].as_str(), found["next"].as_str()) {
        (Some(culprit), _) => println!("  {culprit} broke it: no untested commit is left before it"),
        (None, Some(next)) => println!("  {} left to test; next: {next}", found["remaining"]),
        (None, None) => println!("  nothing to test"),
    }
    Ok(0)
}
