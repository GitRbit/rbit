//! Actions: reusable workflows. A job that calls a workflow becomes that workflow's jobs; the
//! server decides their `if:` with the inputs the call passes (some only known once its
//! needs are done) and gives later jobs the called workflow's outputs. Calling another
//! repository's workflow follows GitHub's access rules.

use rbit_core::{
    PushSource, Timestamp, Visibility,
    actions::{Run, Status},
};
use rbit_services::CreateRepository;

use crate::actions_fixture::{Fixture, git};

const BUILD: &str = "on:
  workflow_call:
    inputs:
      late: {type: string, required: true}
      flavor: {type: string, default: plain}
    outputs:
      result: {value: '${{ jobs.build.outputs.out }}'}
jobs:
  build:
    if: inputs.late == 'go'
    runs-on: x
    outputs: {out: '${{ steps.s.outputs.out }}'}
    steps: [{id: s, run: x}]
";

const CALLER: &str = "on: push
jobs:
  first:
    runs-on: x
    steps: [{run: x}]
  call:
    needs: first
    uses: ./.rbit/workflows/build.yml
    with: {late: '${{ needs.first.outputs.x }}'}
  after:
    needs: call
    if: needs.call.outputs.result == 'built'
    runs-on: x
    steps: [{run: x}]
";

async fn pushed(fixture: &Fixture, files: &[(&str, &str)]) -> Run {
    fixture.commit(files, "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0)
}

async fn jobs(fixture: &Fixture, run: &Run) -> Vec<(String, String, Status)> {
    fixture.database.actions_runs().jobs(run.id).await.unwrap().into_iter().map(|job| (job.key, job.name, job.status)).collect()
}

/// A runner takes the next job and finishes it with `outputs`.
async fn run_next(fixture: &Fixture, seed: u8, outputs: &[(&str, &str)]) -> String {
    let claimed = fixture.claim(&["x"], seed).await.expect("a job to claim");
    let store = fixture.database.actions_runs();
    store.add_outputs(claimed.job.id, outputs, 1024).await.unwrap();
    let settled = store.finish_task(claimed.task.id, Status::Success, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    claimed.job.key
}

#[tokio::test]
async fn called_jobs_get_late_inputs_and_give_their_outputs() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", CALLER), (".rbit/workflows/build.yml", BUILD)]).await;
    assert!(run.error.is_none(), "{:?}", run.error);
    let listed = jobs(&fixture, &run).await;
    let names: Vec<_> = listed.iter().map(|(key, name, _)| (key.as_str(), name.as_str())).collect();
    assert_eq!(names, [("first", "first"), ("call/build", "call / build"), ("after", "after")]);
    assert_eq!(listed[1].2, Status::Blocked, "the called job waits for what its caller waits for");

    assert_eq!(run_next(&fixture, 1, &[("x", "go")]).await, "first");
    // `with: late` read `needs.first.outputs.x`: the called job's `if: inputs.late == 'go'` holds.
    assert_eq!(jobs(&fixture, &run).await[1].2, Status::Waiting);
    assert_eq!(run_next(&fixture, 2, &[("out", "built")]).await, "call/build");
    // `needs.call.outputs.result` is the called workflow's output, over its job's.
    assert_eq!(jobs(&fixture, &run).await[2].2, Status::Waiting);
}

#[tokio::test]
async fn a_false_input_or_caller_condition_skips_the_called_jobs() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", CALLER), (".rbit/workflows/build.yml", BUILD)]).await;
    run_next(&fixture, 1, &[("x", "stop")]).await;
    let statuses: Vec<_> = jobs(&fixture, &run).await.into_iter().map(|(_, _, status)| status).collect();
    assert_eq!(statuses, [Status::Success, Status::Skipped, Status::Skipped], "a skipped call's dependents skip too");

    let skipped_call = "on: push
jobs:
  call:
    if: github.ref == 'refs/heads/other'
    uses: ./.rbit/workflows/build.yml
    with: {late: go}
";
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", skipped_call), (".rbit/workflows/build.yml", BUILD)]).await;
    let statuses: Vec<_> = jobs(&fixture, &run).await.into_iter().map(|(_, _, status)| status).collect();
    assert_eq!(statuses, [Status::Skipped], "the caller's `if` decides first");
}

#[tokio::test]
async fn bad_calls_are_workflow_errors() {
    let fixture = Fixture::new().await;
    let missing_input = "on: push\njobs:\n  call:\n    uses: ./.rbit/workflows/build.yml\n";
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", missing_input), (".rbit/workflows/build.yml", BUILD)]).await;
    assert!(run.error.as_deref().unwrap_or_default().contains("needs the input `late`"), "{:?}", run.error);
    let missing_file = "on: push\njobs:\n  call:\n    uses: ./.rbit/workflows/nope.yml\n";
    let run = pushed(&fixture, &[(".rbit/workflows/ci.yml", missing_file)]).await;
    assert!(run.error.as_deref().unwrap_or_default().contains("is not there, or this repository may not call it"), "{:?}", run.error);
}

#[tokio::test]
async fn private_workflows_are_shared_only_by_their_access_setting() {
    let fixture = Fixture::new().await;
    // alice/rocket is public: make a private caller and a private library.
    for name in ["app", "lib"] {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility: Visibility::Private };
        fixture.services.repositories.create(&fixture.alice, input).await.unwrap();
    }
    let lib = fixture.services.repositories.open(Some(&fixture.alice), "alice", "lib").await.unwrap();
    let app = fixture.services.repositories.open(Some(&fixture.alice), "alice", "app").await.unwrap();
    fixture.database.actions_settings().set_enabled(app.repo.repo.id, true, Timestamp::now()).await.unwrap();
    let shared = "on: workflow_call\njobs:\n  lint:\n    runs-on: x\n    steps: [{run: x}]\n";
    let caller = "on: push\njobs:\n  shared:\n    uses: alice/lib/.rbit/workflows/lint.yml@main\n";
    let push = |repo: &rbit_services::RepoContext, files: &[(&str, &str)]| {
        let work = fixture.dir.path().join(format!("work-{}", repo.repo.repo.name));
        if !work.exists() {
            std::fs::create_dir(&work).unwrap();
            git(&work, &["init", "-q"]);
        }
        for (path, contents) in files {
            let path = work.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-q", "--allow-empty", "-m", "x"]);
        git(&work, &["push", "-q", "-f", repo.disk_path.to_str().unwrap(), "HEAD:refs/heads/main"]);
        rbit_git::RefUpdate { name: "refs/heads/main".into(), before: None, after: Some(git(&work, &["rev-parse", "HEAD"])) }
    };
    push(&lib, &[(".rbit/workflows/lint.yml", shared)]);
    let update = push(&app, &[(".rbit/workflows/ci.yml", caller)]);
    let actions = fixture.actions();
    let source = PushSource::User(fixture.alice.id());
    let denied = actions.handle_push(app.repo.repo.id, source, std::slice::from_ref(&update)).await.unwrap().remove(0);
    assert!(denied.error.as_deref().unwrap_or_default().contains("may not call it"), "{:?}", denied.error);

    actions.set_repo_access_level(&fixture.alice, &lib, "user").await.unwrap();
    let allowed = actions.handle_push(app.repo.repo.id, source, &[update]).await.unwrap().remove(0);
    assert!(allowed.error.is_none(), "{:?}", allowed.error);
    let listed = jobs(&fixture, &allowed).await;
    assert_eq!(listed[0].1, "shared / lint");
    assert!(actions.set_repo_access_level(&fixture.alice, &lib, "everyone").await.is_err());
}
