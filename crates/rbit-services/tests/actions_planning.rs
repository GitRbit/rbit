//! Actions join (J): runs are planned with the `github` context a task gets — the commit,
//! the actors, the ref and the server — plus dispatch inputs and, for trusted runs only, the
//! variables.

use std::collections::BTreeMap;

use rbit_core::{PushSource, actions::SecretScope};
use rbit_services::actions::dispatch::DispatchRequest;

use crate::actions_fixture::Fixture;

const NAMED: &str = "on: [push, workflow_dispatch]\njobs:\n  build:\n    name: \"${{ github.actor }}@${{ github.ref_name }} ${{ github.sha }} ${{ vars.REGION }} [${{ inputs.target }}] ${{ github.server_url }}\"\n    runs-on: x\n    steps:\n      - run: echo hi\n";

async fn job_name(fixture: &Fixture, run: rbit_core::actions::RunId) -> String {
    fixture.database.actions_runs().jobs(run).await.unwrap().remove(0).name
}

#[tokio::test]
async fn pushes_and_dispatches_plan_with_the_whole_context() {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice).await;
    let scope = SecretScope::Repository(repo.repo.repo.id);
    let region = "region".parse().unwrap();
    fixture
        .database
        .actions_secrets()
        .put_variable(scope, &region, "eu-west", fixture.alice.id(), rbit_core::Timestamp::now())
        .await
        .unwrap();
    let sha = fixture.commit(&[(".rbit/workflows/ci.yml", NAMED)], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let actions = fixture.actions();
    let run = actions.handle_push(repo.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    assert_eq!(job_name(&fixture, run.id).await, format!("alice@main {sha} eu-west [] https://git.example.com"));

    let writer = fixture.repo(&fixture.carol).await;
    let request = DispatchRequest {
        workflow: "ci.yml".into(),
        reference: "main".into(),
        inputs: BTreeMap::from([("target".to_owned(), "arm64".to_owned())]),
    };
    // The workflow declares no inputs, so the dispatch takes none.
    assert!(actions.dispatch(&fixture.carol, &writer, &request).await.is_err());
    let with_inputs = NAMED.replacen(
        "on: [push, workflow_dispatch]\n",
        "on:\n  push:\n  workflow_dispatch:\n    inputs:\n      target: {type: string}\n",
        1,
    );
    let sha = fixture.commit(&[(".rbit/workflows/ci.yml", &with_inputs)], "Inputs");
    fixture.push_ref("refs/heads/main").await;
    let dispatched = actions.dispatch(&fixture.carol, &writer, &request).await.unwrap();
    assert_eq!(job_name(&fixture, dispatched.id).await, format!("carol@main {sha} eu-west [arm64] https://git.example.com"));
}
