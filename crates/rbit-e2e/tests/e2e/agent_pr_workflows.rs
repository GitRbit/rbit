//! Native agent commands against a real repository and PR.

use rbit_e2e::{Git, PASSWORD, TestServer};
use reqwest::Method;
use serde_json::{Value, json};

use super::agent_runs::Agent;

const PULL: &str = "repos/alice/rocket/pulls/1";
const QUEUE: &str = "repos/alice/rocket/pulls/1/queue";

async fn assert_open_and_unqueued(agent: &Agent<'_>, base: &str, head: &str) {
    let (status, pull) = agent.get(PULL).await;
    assert_eq!(status, 200, "{pull}");
    assert_eq!(pull["state"], "open", "{pull}");
    assert_eq!(pull["head"]["sha"], head, "{pull}");
    assert_eq!(pull["base"]["sha"], base, "{pull}");
    let (status, queue) = agent.get(QUEUE).await;
    assert_eq!(status, 404, "no queue entry should exist: {queue}");
}

fn assert_evidence(evidence: &Value, schema: &str, base: &str, head: &str) {
    assert_eq!(evidence["schema"], schema, "{evidence}");
    assert_eq!(evidence["pull"]["number"], 1, "{evidence}");
    assert_eq!(evidence["comparison"]["base"], base, "{evidence}");
    assert_eq!(evidence["comparison"]["head"], head, "{evidence}");
    assert_eq!(evidence["comparison"]["ahead"], 1, "{evidence}");
    assert_eq!(evidence["comparison"]["behind"], 1, "{evidence}");
    assert_eq!(evidence["manifest"]["files"][0]["path"], "landing.txt", "{evidence}");
}

#[tokio::test]
async fn agent_pr_workflows_report_evidence_and_only_land_on_explicit_send() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "rocket", "private").await;
    let owner = Agent::new(&server, alice.create_token("workflow writer", "admin").await);
    let reader = Agent::new(&server, alice.create_token("workflow reader", "read").await);
    let reviewer = Agent::new(&server, bob.create_token("workflow reviewer", "write").await);
    let (status, result) = owner.send(Method::PUT, "repos/alice/rocket/collaborators/bob", Some(json!({ "permission": "push" }))).await;
    assert_eq!(status, 201, "{result}");

    let git = Git::new();
    let sample = git.sample();
    let remote = server.git_url_as("alice", "rocket", "alice", PASSWORD);
    git.commit_file(&sample.dir, "rocket.txt", "v1\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "main"]);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "maintenance"]);
    let base = git.commit_file(&sample.dir, "fuel.txt", "more\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "maintenance"]);
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "landing", "main"]);
    let head = git.commit_file(&sample.dir, "landing.txt", "legs\n");
    git.ok(&sample.dir, &["push", "--quiet", &remote, "landing"]);
    let pull =
        json!({ "title": "Add a backup landing path", "body": "Deploy legs before contact.", "head": "landing", "base": "maintenance" });
    let (status, pull) = owner.send(Method::POST, "repos/alice/rocket/pulls", Some(pull)).await;
    assert_eq!(status, 201, "{pull}");
    assert_eq!(pull["number"], 1, "{pull}");

    let (code, modernization) = owner.cli(&["pr", "modernize", "1"]);
    assert_eq!(code, 0, "{modernization}");
    assert_evidence(&modernization, "rbit.pr-modernize/1", &base, &head);
    assert_eq!(modernization["disposition"], "needs_assessment", "{modernization}");

    let (code, takeover) = reader.cli(&["pr", "takeover", "1"]);
    assert_eq!(code, 0, "{takeover}");
    assert_evidence(&takeover, "rbit.pr-takeover/1", &base, &head);
    assert_eq!(takeover["watch"]["head"], head, "{takeover}");
    assert_eq!(takeover["watch"]["state"], "open", "{takeover}");
    assert_open_and_unqueued(&owner, &base, &head).await;

    let (code, blocked) = reader.cli(&["pr", "full-send", "1", "--head", &head, "--timeout", "0"]);
    assert_eq!((code, blocked["code"].as_str()), (3, Some("forbidden")), "{blocked}");
    let (code, stale) = owner.cli(&["pr", "full-send", "1", "--head", &base, "--timeout", "0"]);
    assert_eq!((code, stale["code"].as_str()), (5, Some("head_changed")), "{stale}");
    let (status, stale) = owner.send(Method::POST, QUEUE, Some(json!({ "head": head, "base": "main" }))).await;
    assert_eq!((status, stale["code"].as_str()), (409, Some("base_changed")), "{stale}");
    assert_open_and_unqueued(&owner, &base, &head).await;

    let comment = json!({ "path": "landing.txt", "line": 1, "body": "What happens if the backup sensor fails?" });
    let (status, comment) = reviewer.send(Method::POST, &format!("{PULL}/comments"), Some(comment)).await;
    assert_eq!(status, 201, "{comment}");
    let comment_id = comment["id"].as_i64().unwrap().to_string();
    let reply_body = "The mechanical fallback deploys the legs.";
    let (code, reply) = owner.cli(&["pr", "comment", "1", "--reply-to", &comment_id, "--body", reply_body]);
    assert_eq!(code, 0, "{reply}");
    assert_eq!(reply["schema"], "rbit.pr-comment/1", "{reply}");
    assert_eq!(reply["in_reply_to_id"], comment["id"], "{reply}");
    assert_eq!(reply["body"], reply_body, "{reply}");
    let (code, blocked) = owner.cli(&["pr", "babysit", "1", "--timeout", "5"]);
    assert_eq!(code, 1, "{blocked}");
    assert_eq!(blocked["schema"], "rbit.pr-babysit/1", "{blocked}");
    assert_eq!(blocked["head"], head, "{blocked}");
    assert_eq!(blocked["thread_count"], 1, "{blocked}");
    let (code, blocked) = owner.cli(&["pr", "full-send", "1", "--head", &head, "--timeout", "0"]);
    assert_eq!((code, blocked["code"].as_str()), (1, Some("blocked")), "{blocked}");
    assert_open_and_unqueued(&owner, &base, &head).await;

    let resolution = json!({ "resolved": true });
    let (status, result) = reviewer.send(Method::PUT, &format!("{PULL}/comments/{comment_id}/resolution"), Some(resolution)).await;
    assert_eq!(status, 200, "{result}");
    let (code, handoff) = owner.cli(&["pr", "full-stop", "1", "--timeout", "10"]);
    assert_eq!(code, 0, "{handoff}");
    assert_eq!(handoff["outcome"], "ready", "{handoff}");
    assert_eq!(handoff["head"], head, "{handoff}");
    assert_open_and_unqueued(&owner, &base, &head).await;

    let (code, landed) = owner.cli(&["pr", "full-send", "1", "--head", &head, "--timeout", "20"]);
    assert_eq!(code, 0, "{landed}");
    assert_eq!(landed["schema"], "rbit.queue/1", "{landed}");
    assert_eq!(landed["state"], "landed", "{landed}");
    let (status, merged) = owner.get(PULL).await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(merged["merged"], true, "{merged}");
    assert_eq!(merged["merge_commit_sha"], landed["candidate"], "{merged}");
    let (status, tip) = owner.get("repos/alice/rocket/commits/maintenance").await;
    assert_eq!(status, 200, "{tip}");
    assert_eq!(tip["sha"], landed["candidate"], "the queue's candidate is the new base: {landed}");

    let (status, tip) = owner.get("repos/alice/rocket/commits/landing").await;
    assert_eq!(status, 200, "{tip}");
    assert_eq!(tip["sha"], head, "the source branch remains at its original revision");
}
