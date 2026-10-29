//! Actions workstream T: pushes start workflow runs — discovery, trigger filters, skip
//! markers, where a push came from, workflow errors, the per-event and per-repository caps
//! and concurrency groups — against real stores and repositories filled with `git`.

use std::time::Duration;

use rbit_core::{
    PushSource, RefUpdate, Timestamp,
    actions::{RunEvent, Status, TaskId},
};
use serde_json::Value;

use crate::actions_fixture::{Fixture, workflow};

const CI: &str = ".forgejo/workflows/ci.yml";

/// Pushes `files` to `reference` and lets `source` report it.
async fn push(
    fixture: &Fixture,
    files: &[(&str, &str)],
    message: &str,
    reference: &str,
    source: PushSource,
) -> Vec<rbit_core::actions::Run> {
    fixture.commit(files, message);
    let update = fixture.push_ref(reference).await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, source, &[update]).await.unwrap()
}

fn user_push(fixture: &Fixture) -> PushSource {
    PushSource::User(fixture.alice.id())
}

#[tokio::test]
async fn a_push_starts_a_run_with_its_jobs_and_event() {
    let fixture = Fixture::new().await;
    let matrix = "name: CI\non: push\njobs:\n  test:\n    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n        os: [linux, windows]\n    steps:\n      - run: echo\n  deploy:\n    needs: test\n    runs-on: linux\n    permissions:\n      contents: write\n    steps:\n      - run: echo\n";
    let source = user_push(&fixture);
    let runs = push(
        &fixture,
        &[(CI, matrix), (".github/workflows/ignored.yml", &workflow("Ignored", "push"))],
        "Add CI",
        "refs/heads/main",
        source,
    )
    .await;
    assert_eq!(runs.len(), 1, "only .forgejo/workflows is read when it exists: {runs:?}");
    let run = &runs[0];
    assert_eq!((run.workflow_path.as_str(), run.workflow_name.as_str(), run.title.as_str()), (CI, "CI", "Add CI"));
    assert_eq!((run.event, run.reference.as_str(), run.number, run.attempt), (RunEvent::Push, "refs/heads/main", 1, 1));
    assert_eq!((run.actor, run.trigger_actor, run.trusted, run.is_fork), (Some(fixture.alice.id()), Some(fixture.alice.id()), true, false));
    assert_eq!(run.status, Status::Waiting);

    let store = fixture.database.actions_runs();
    let jobs = store.jobs(run.id).await.unwrap();
    let summary: Vec<_> = jobs.iter().map(|job| (job.key.as_str(), job.name.as_str(), job.status, job.runs_on.clone())).collect();
    assert_eq!(
        summary,
        [
            ("test", "test (linux)", Status::Waiting, vec!["linux".to_owned()]),
            ("test", "test (windows)", Status::Waiting, vec!["windows".to_owned()]),
            ("deploy", "deploy", Status::Blocked, vec!["linux".to_owned()]),
        ]
    );
    assert_eq!(jobs[2].permissions.as_deref(), Some(r#"{"contents":"write","issues":"none","pull_requests":"none"}"#));
    let payload = String::from_utf8(store.job_payload(jobs[0].id).await.unwrap().unwrap()).unwrap();
    assert!(payload.contains("test"), "{payload}");

    let (event, inputs) = store.event(run.id).await.unwrap().unwrap();
    let event: Value = serde_json::from_str(&event).unwrap();
    assert_eq!((event["ref"].as_str(), event["after"].as_str()), (Some("refs/heads/main"), Some(run.sha.as_str())));
    assert_eq!(event["head_commit"]["message"], "Add CI");
    assert_eq!(event["repository"]["full_name"], "alice/rocket");
    assert_eq!(event["sender"]["login"], "alice");
    assert_eq!(inputs, None);

    let context = fixture.actions().task_context(run, &jobs[0]).await.unwrap();
    assert_eq!(
        (context["event_name"].as_str(), context["actor"].as_str(), context["job"].as_str()),
        (Some("push"), Some("alice"), Some("test"))
    );
    assert_eq!((context["ref_name"].as_str(), context["run_number"].as_str()), (Some("main"), Some("1")));
    assert_eq!(context["event"]["head_commit"]["message"], "Add CI");
    assert_eq!(context["api_url"], "https://git.example.com/api/v1");
    assert!(context.get("token").is_none(), "the scheduler adds the token");
}

#[tokio::test]
async fn trigger_filters_decide_which_pushes_start_runs() {
    let fixture = Fixture::new().await;
    let workflows = [
        (".forgejo/workflows/any.yml", workflow("Any", "push")),
        (".forgejo/workflows/main.yml", workflow("Main only", "{ push: { branches: [main] } }")),
        (".forgejo/workflows/release.yml", workflow("Tags", "{ push: { tags: ['v*'] } }")),
        (".forgejo/workflows/docs.yml", workflow("Docs", "{ push: { paths: ['docs/**'] } }")),
        (".forgejo/workflows/not-docs.yml", workflow("Not docs", "{ push: { paths-ignore: ['docs/**'] } }")),
        (".forgejo/workflows/pr.yml", workflow("Pulls", "pull_request")),
        (".forgejo/workflows/manual.yml", workflow("Manual", "workflow_dispatch")),
    ];
    let files: Vec<(&str, &str)> = workflows.iter().map(|(path, body)| (*path, body.as_str())).collect();
    let names = |runs: &[rbit_core::actions::Run]| {
        let mut names: Vec<String> = runs.iter().map(|run| run.workflow_name.clone()).collect();
        names.sort();
        names
    };
    let source = user_push(&fixture);
    // The first push adds every file, including docs/.
    let mut first = files.clone();
    first.push(("docs/guide.md", "# Guide\n"));
    let runs = push(&fixture, &first, "Add workflows", "refs/heads/main", source).await;
    assert_eq!(names(&runs), ["Any", "Docs", "Main only", "Not docs"]);

    let runs = push(&fixture, &[("docs/guide.md", "# Guide, again\n")], "Docs only", "refs/heads/main", source).await;
    assert_eq!(names(&runs), ["Any", "Docs", "Main only"], "only docs changed");

    let runs = push(&fixture, &[("src/lib.rs", "// code\n")], "Code", "refs/heads/feature", source).await;
    assert_eq!(names(&runs), ["Any", "Not docs"], "a new branch is compared with the default branch; not main");

    fixture.commit(&[], "Release");
    let tag = fixture.push_ref("refs/tags/v1.0").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let runs = fixture.actions().handle_push(repo, source, std::slice::from_ref(&tag)).await.unwrap();
    assert_eq!(names(&runs), ["Any", "Docs", "Not docs", "Tags"], "tag pushes skip paths filters; branch filters never match tags");
    assert!(runs.iter().all(|run| run.reference == "refs/tags/v1.0"));

    let deleted = RefUpdate { name: "refs/heads/feature".into(), before: tag.after.clone(), after: None };
    assert!(fixture.actions().handle_push(repo, source, &[deleted]).await.unwrap().is_empty(), "deletions start nothing");
    let other = RefUpdate { name: "refs/notes/commits".into(), before: None, after: tag.after.clone() };
    assert!(fixture.actions().handle_push(repo, source, &[other]).await.unwrap().is_empty(), "neither a branch nor a tag");
}

#[tokio::test]
async fn a_force_push_of_an_unrelated_history_passes_paths_filters() {
    let fixture = Fixture::new().await;
    let docs = workflow("Docs", "{ push: { paths: ['docs/**'] } }");
    let source = user_push(&fixture);
    push(&fixture, &[(".forgejo/workflows/docs.yml", &docs)], "Add the workflow", "refs/heads/main", source).await;
    // A new root commit shares nothing with what main held: what changed is unknown.
    crate::actions_fixture::git(&fixture.work, &["checkout", "-q", "--orphan", "rewrite"]);
    let runs =
        push(&fixture, &[(".forgejo/workflows/docs.yml", &docs), ("docs/new.md", "# New\n")], "Rewrite", "refs/heads/main", source).await;
    assert_eq!(runs.iter().map(|run| run.workflow_name.as_str()).collect::<Vec<_>>(), ["Docs"]);
}

#[tokio::test]
async fn one_push_of_several_refs_starts_a_run_per_ref() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(CI, &workflow("CI", "push"))], "CI");
    let updates =
        [fixture.push_ref("refs/heads/main").await, fixture.push_ref("refs/heads/one").await, fixture.push_ref("refs/heads/two").await];
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let runs = fixture.actions().handle_push(repo, user_push(&fixture), &updates).await.unwrap();
    let refs: Vec<_> = runs.iter().map(|run| run.reference.as_str()).collect();
    assert_eq!(refs, ["refs/heads/main", "refs/heads/one", "refs/heads/two"]);
    let numbers: Vec<_> = runs.iter().map(|run| run.number).collect();
    assert_eq!(numbers, [1, 2, 3]);
}

#[tokio::test]
async fn skip_markers_skip_the_push() {
    let fixture = Fixture::new().await;
    let source = user_push(&fixture);
    for marker in ["[skip ci]", "[ci skip]", "[no ci]", "[skip actions]", "[actions skip]"] {
        let runs = push(&fixture, &[(CI, &workflow("CI", "push"))], &format!("Tidy {marker}"), "refs/heads/main", source).await;
        assert!(runs.is_empty(), "{marker}");
    }
    fixture.commit(&[], "Subject");
    git_amend(&fixture, "Subject\n\nBody with [skip ci] in it");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    assert!(fixture.actions().handle_push(repo, source, &[update]).await.unwrap().is_empty(), "the body counts too");
    assert_eq!(push(&fixture, &[], "Now run", "refs/heads/main", source).await.len(), 1);
}

fn git_amend(fixture: &Fixture, message: &str) {
    crate::actions_fixture::git(&fixture.work, &["commit", "-q", "--amend", "--allow-empty", "-m", message]);
}

#[tokio::test]
async fn only_people_and_deploy_keys_start_runs() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(CI, &workflow("CI", "push"))], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let actions = fixture.actions();
    for source in [PushSource::Mirror, PushSource::Import, PushSource::Task(TaskId(1)), PushSource::System] {
        assert!(actions.handle_push(repo, source, std::slice::from_ref(&update)).await.unwrap().is_empty(), "{source:?}");
    }
    assert!(fixture.runs().await.is_empty(), "nothing was written");
    let runs = actions.handle_push(repo, PushSource::DeployKey, std::slice::from_ref(&update)).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].actor, runs[0].trusted), (None, true), "a deploy key has no account");
}

#[tokio::test]
async fn a_repository_with_actions_off_starts_nothing() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.database.actions_settings().set_enabled(repo, false, Timestamp::now()).await.unwrap();
    let runs = push(&fixture, &[(CI, &workflow("CI", "push"))], "CI", "refs/heads/main", user_push(&fixture)).await;
    assert!(runs.is_empty(), "{runs:?}");
    assert!(fixture.runs().await.is_empty(), "nothing was written");
}

#[tokio::test]
async fn broken_workflows_are_shown_as_workflow_errors() {
    let fixture = Fixture::new().await;
    let files = [
        (".forgejo/workflows/a-broken.yml", "on: push\njobs: [not, a, mapping]\n"),
        (
            ".forgejo/workflows/b-cycle.yml",
            "on: push\njobs:\n  a:\n    needs: b\n    runs-on: x\n    steps: [{run: x}]\n  b:\n    needs: a\n    runs-on: x\n    steps: [{run: x}]\n",
        ),
        (".forgejo/workflows/c-good.yml", "on: push\njobs:\n  ok:\n    runs-on: x\n    steps: [{run: x}]\n"),
        (".forgejo/workflows/readme.md", "not a workflow"),
    ];
    let runs = push(&fixture, &files, "Workflows", "refs/heads/main", user_push(&fixture)).await;
    let summary: Vec<_> = runs.iter().map(|run| (run.workflow_path.as_str(), run.status, run.error.is_some())).collect();
    assert_eq!(
        summary,
        [
            (".forgejo/workflows/a-broken.yml", Status::Failure, true),
            (".forgejo/workflows/b-cycle.yml", Status::Failure, true),
            (".forgejo/workflows/c-good.yml", Status::Waiting, false),
        ]
    );
    let error = runs[0].error.as_deref().unwrap();
    assert!(error.starts_with(".forgejo/workflows/a-broken.yml: line 2"), "{error}");
    assert!(fixture.database.actions_runs().jobs(runs[0].id).await.unwrap().is_empty(), "a workflow error has no jobs");
    assert_eq!(runs[2].workflow_name, ".forgejo/workflows/c-good.yml", "a workflow without a name is called by its path");

    let large = format!("on: push\njobs:\n  ok:\n    runs-on: x\n    steps: [{{run: x}}]\n# {}\n", "x".repeat(300 * 1024));
    let runs = push(&fixture, &[(".forgejo/workflows/c-good.yml", &large)], "Large", "refs/heads/main", user_push(&fixture)).await;
    let errors: Vec<_> = runs.iter().filter_map(|run| run.error.as_deref()).collect();
    assert!(errors.iter().any(|error| error.contains("larger than 262144 bytes")), "{errors:?}");
}

#[tokio::test]
async fn an_event_starts_at_most_its_share_of_runs_and_jobs() {
    let fixture = Fixture::with(|config| {
        config.actions.max_runs_per_event = 2;
        config.actions.max_jobs_per_event = 3;
    })
    .await;
    let two_jobs = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n  b:\n    runs-on: x\n    steps: [{run: x}]\n";
    let files = [(".forgejo/workflows/1.yml", two_jobs), (".forgejo/workflows/2.yml", two_jobs), (".forgejo/workflows/3.yml", two_jobs)];
    let runs = push(&fixture, &files, "Many", "refs/heads/main", user_push(&fixture)).await;
    assert_eq!(runs.len(), 2, "at most two runs");
    assert_eq!(runs[0].status, Status::Waiting);
    let error = runs[1].error.as_deref().unwrap();
    assert!(error.contains("one event may start at most 3"), "{error}");
}

#[tokio::test]
async fn a_repository_may_have_only_so_many_jobs_waiting() {
    let fixture = Fixture::with(|config| config.actions.max_waiting_jobs_per_repository = 3).await;
    let two_jobs = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n  b:\n    runs-on: x\n    steps: [{run: x}]\n  c:\n    needs: a\n    runs-on: x\n    steps: [{run: x}]\n";
    let source = user_push(&fixture);
    let first = push(&fixture, &[(CI, two_jobs)], "One", "refs/heads/main", source).await;
    assert_eq!(first[0].status, Status::Waiting);
    let second = push(&fixture, &[], "Two", "refs/heads/main", source).await;
    let error = second[0].error.as_deref().unwrap();
    assert!(error.contains("already has 2 jobs waiting") && error.contains("at most 3"), "{error}");
}

#[tokio::test]
async fn concurrency_groups_cancel_or_hold_earlier_runs() {
    let fixture = Fixture::new().await;
    let source = user_push(&fixture);
    let cancelling = "name: Deploy\non: push\nconcurrency:\n  group: deploy-${{ github.ref }}\n  cancel-in-progress: true\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let first = push(&fixture, &[(CI, cancelling)], "One", "refs/heads/main", source).await.remove(0);
    assert_eq!(first.concurrency_group.as_deref(), Some("deploy-refs/heads/main"));
    let second = push(&fixture, &[], "Two", "refs/heads/main", source).await.remove(0);
    let store = fixture.database.actions_runs();
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    assert_eq!(store.find_run(repo, first.id).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(second.status, Status::Waiting);
    let other_branch = push(&fixture, &[], "Three", "refs/heads/other", source).await.remove(0);
    assert_eq!((other_branch.status, store.find_run(repo, second.id).await.unwrap().unwrap().status), (Status::Waiting, Status::Waiting));

    let queueing = "name: Queue\non: push\nconcurrency: queue\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let path = ".forgejo/workflows/queue.yml";
    let active = push(&fixture, &[(CI, ""), (path, queueing)], "Queue one", "refs/heads/main", source).await.remove(0);
    let held = push(&fixture, &[], "Queue two", "refs/heads/main", source).await.remove(0);
    assert_eq!((active.status, held.status), (Status::Waiting, Status::Blocked), "held while the group is busy");
    let newer = push(&fixture, &[], "Queue three", "refs/heads/main", source).await.remove(0);
    assert_eq!(newer.status, Status::Blocked);
    assert_eq!(store.find_run(repo, held.id).await.unwrap().unwrap().status, Status::Cancelled, "the newer pending run replaces it");

    // The active run finishing lets the held one start.
    let settled = store.cancel_run(repo, active.id, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    assert_eq!(store.find_run(repo, newer.id).await.unwrap().unwrap().status, Status::Waiting);

    let unique = "on: push\nconcurrency: ci-${{ github.run_id }}\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let run = push(&fixture, &[(path, ""), (CI, unique)], "Unique", "refs/heads/main", source).await.remove(0);
    assert_eq!(run.concurrency_group, None, "a group naming the run id holds nothing");
    let numbered = "on: push\nconcurrency: ci-${{ github.run_number }}\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let run = push(&fixture, &[(CI, numbered)], "Numbered", "refs/heads/main", source).await.remove(0);
    assert_eq!(run.concurrency_group, None, "nor does one naming the run number");

    // Jobs' own groups follow the same rule.
    let jobs = "on: push\njobs:\n  unique:\n    runs-on: x\n    concurrency: build-${{ github.run_id }}\n    steps: [{run: x}]\n  \
                numbered:\n    runs-on: x\n    concurrency: build-${{ github.run_number }}\n    steps: [{run: x}]\n  \
                shared:\n    runs-on: x\n    concurrency: build-${{ github.ref }}\n    steps: [{run: x}]\n";
    let run = push(&fixture, &[(CI, jobs)], "Job groups", "refs/heads/main", source).await.remove(0);
    let mut groups = Vec::new();
    for job in store.jobs(run.id).await.unwrap() {
        let payload: serde_json::Value = serde_json::from_slice(&store.job_payload(job.id).await.unwrap().unwrap()).unwrap();
        groups.push((job.key, payload["concurrency"]["group"].as_str().map(str::to_owned)));
    }
    groups.sort();
    assert_eq!(groups, [("numbered".into(), None), ("shared".into(), Some("build-refs/heads/main".into())), ("unique".into(), None)]);
}

#[tokio::test]
async fn the_subscriber_starts_runs_and_records_new_repositories() {
    let fixture = Fixture::with(|config| config.actions.enable_for_new_repositories = true).await;
    fixture.services.spawn_background();
    let input =
        rbit_services::CreateRepository { name: "fresh".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
    let fresh = fixture.services.repositories.create(&fixture.bob, input).await.unwrap();

    fixture.commit(&[(CI, &workflow("CI", "push"))], "CI");
    let repo = fixture.repo(&fixture.alice).await;
    let pending = fixture.services.pushes.begin(&repo, PushSource::User(fixture.alice.id())).await;
    let path = repo.disk_path.to_str().unwrap().to_owned();
    crate::actions_fixture::git(&fixture.work, &["push", "-q", &path, "HEAD:refs/heads/main"]);
    fixture.services.pushes.complete(pending).await;
    let mut waited = Duration::ZERO;
    while fixture.runs().await.is_empty() {
        assert!(waited < Duration::from_secs(20), "the push started no run");
        tokio::time::sleep(Duration::from_millis(50)).await;
        waited += Duration::from_millis(50);
    }
    assert!(fixture.database.actions_settings().is_enabled(fresh.repo.id).await.unwrap(), "new repositories follow the configuration");
    assert_eq!(fixture.actions().missed_events(), 0);
    fixture.services.shutdown().await;
}
