//! Actions workstream T: people acting on runs — cancel, re-run (all, failed, one job)
//! and approval — with their permission checks, the audit log and concurrency groups
//! applied when a run becomes runnable (§11.6).

use rbit_core::{
    IssueId, PushSource, Timestamp, User,
    actions::{JobId, Run, RunId, Status},
    issue::IssueAction,
    pull::HeadSpec,
};
use rbit_services::{ServiceError, audit::AuditAction, forks::ForkInput, pulls::NewPullInput};
use rbit_storage::sqlite::actions_runs::Rerun;

use crate::actions_fixture::{Fixture, git};

const CHAIN: &str =
    "name: CI\non: [push, pull_request]\njobs:\n  a:\n    runs-on: x\n  b:\n    needs: a\n    runs-on: x\n  c:\n    runs-on: y\n";

async fn pushed(fixture: &Fixture, files: &[(&str, &str)], message: &str) -> Run {
    fixture.commit(files, message);
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0)
}

async fn statuses(fixture: &Fixture, run: RunId) -> Vec<(String, Status)> {
    let jobs = fixture.database.actions_runs().jobs(run).await.unwrap();
    jobs.into_iter().map(|job| (job.key, job.status)).collect()
}

fn keyed(pairs: &[(&str, Status)]) -> Vec<(String, Status)> {
    pairs.iter().map(|(key, status)| ((*key).to_owned(), *status)).collect()
}

#[tokio::test]
async fn writers_cancel_and_run_again() {
    let fixture = Fixture::new().await;
    let run = pushed(&fixture, &[(".forgejo/workflows/ci.yml", CHAIN)], "CI").await;
    let actions = fixture.actions();
    let (writer, reader, stranger) =
        (fixture.repo(&fixture.carol).await, fixture.repo(&fixture.rita).await, fixture.repo(&fixture.bob).await);
    assert!(matches!(actions.cancel(&fixture.rita, &reader, run.id).await, Err(ServiceError::Forbidden)));
    assert!(
        matches!(actions.cancel(&fixture.bob, &stranger, run.id).await, Err(ServiceError::Forbidden)),
        "public: readable, not writable"
    );
    assert!(matches!(actions.rerun(&fixture.carol, &writer, run.id, Rerun::All).await, Err(ServiceError::Invalid(_))), "not finished yet");
    assert!(matches!(actions.cancel(&fixture.carol, &writer, RunId(9_999)).await, Err(ServiceError::NotFound)));

    // `a` runs and fails; cancelling then stops everything that has not finished.
    let claimed = fixture.claim(&["x"], 1).await.unwrap();
    assert_eq!(claimed.job.key, "a");
    let store = fixture.database.actions_runs();
    let settled = store.finish_task(claimed.task.id, Status::Failure, Timestamp::now()).await.unwrap().unwrap();
    actions.settled(&settled).await.unwrap();
    assert_eq!(statuses(&fixture, run.id).await, keyed(&[("a", Status::Failure), ("b", Status::Waiting), ("c", Status::Waiting)]));
    let cancelled = actions.cancel(&fixture.carol, &writer, run.id).await.unwrap();
    assert_eq!(cancelled.status, Status::Cancelled);
    assert_eq!(statuses(&fixture, run.id).await, keyed(&[("a", Status::Failure), ("b", Status::Cancelled), ("c", Status::Cancelled)]));
    assert!(matches!(actions.cancel(&fixture.carol, &writer, run.id).await, Err(ServiceError::Invalid(_))), "already finished");

    let version = actions.tasks_version();
    let again = actions.rerun(&fixture.carol, &writer, run.id, Rerun::Failed).await.unwrap();
    assert_eq!((again.attempt, again.number, again.status), (2, run.number, Status::Waiting));
    assert!(actions.tasks_version() > version, "jobs became claimable");
    assert_eq!(statuses(&fixture, run.id).await, keyed(&[("a", Status::Waiting), ("b", Status::Blocked), ("c", Status::Waiting)]));

    actions.cancel(&fixture.alice, &fixture.repo(&fixture.alice).await, run.id).await.unwrap();
    let jobs = store.jobs(run.id).await.unwrap();
    let again = actions.rerun(&fixture.carol, &writer, run.id, Rerun::Job(jobs[2].id)).await.unwrap();
    assert_eq!(again.attempt, 3);
    assert_eq!(statuses(&fixture, run.id).await, keyed(&[("a", Status::Cancelled), ("b", Status::Cancelled), ("c", Status::Waiting)]));
    actions.cancel(&fixture.carol, &writer, run.id).await.unwrap();
    assert!(matches!(actions.rerun(&fixture.carol, &writer, run.id, Rerun::Job(JobId(9_999))).await, Err(ServiceError::NotFound)));
    let other = pushed(&fixture, &[], "Other").await;
    let other_job = store.jobs(other.id).await.unwrap()[0].id;
    assert!(
        matches!(actions.rerun(&fixture.carol, &writer, run.id, Rerun::Job(other_job)).await, Err(ServiceError::NotFound)),
        "another run's job"
    );
    let all = actions.rerun(&fixture.carol, &writer, run.id, Rerun::All).await.unwrap();
    assert_eq!(all.attempt, 4);
    assert!(matches!(actions.rerun(&fixture.rita, &reader, run.id, Rerun::All).await, Err(ServiceError::Forbidden)));

    let broken = pushed(&fixture, &[(".forgejo/workflows/ci.yml", "on: push\njobs: 3\n")], "Broken").await;
    assert!(matches!(actions.rerun(&fixture.carol, &writer, broken.id, Rerun::All).await, Err(ServiceError::Invalid(_))));
}

/// `alice/rocket` runs `workflow` on pull requests; bob opens one from his fork.
async fn fork_pull(fixture: &Fixture, workflow: &str) -> (IssueId, Run) {
    fixture.commit(&[(".forgejo/workflows/ci.yml", workflow)], "CI");
    fixture.push_ref("refs/heads/main").await;
    let parent = fixture.repo(&fixture.bob).await;
    fixture.services.forks.fork(&fixture.bob, &parent, ForkInput::default()).await.unwrap();
    let fork = fixture.services.repositories.open(Some(&fixture.bob), "bob", "rocket").await.unwrap();
    fixture.commit(&[("fix.txt", "fix\n")], "Fix");
    git(&fixture.work, &["push", "-q", fork.disk_path.to_str().unwrap(), "HEAD:refs/heads/fix"]);
    let base = fixture.repo(&fixture.bob).await;
    let head = fixture.services.pulls.resolve_head(Some(&fixture.bob), &base, &HeadSpec::parse("bob:fix").unwrap()).await.unwrap();
    let input = NewPullInput { base_branch: "main".into(), head_branch: "fix".into(), title: "Fix".into(), ..NewPullInput::default() };
    let issue = fixture.services.pulls.create(&fixture.bob, &base, &head, input).await.unwrap().0.id;
    let run = opened(fixture, issue, &fixture.bob).await;
    (issue, run)
}

async fn opened(fixture: &Fixture, issue: IssueId, by: &User) -> Run {
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_pull_request(repo, issue, Some(by.id()), &IssueAction::Opened).await.unwrap().remove(0)
}

#[tokio::test]
async fn writers_approve_untrusted_runs_and_may_trust_their_author() {
    let fixture = Fixture::new().await;
    let (issue, run) = fork_pull(&fixture, CHAIN).await;
    assert_eq!(run.status, Status::AwaitingApproval);
    let actions = fixture.actions();
    let reader = fixture.repo(&fixture.rita).await;
    assert!(matches!(actions.approve(&fixture.rita, &reader, run.id, false).await, Err(ServiceError::Forbidden)));
    let writer = fixture.repo(&fixture.carol).await;
    let version = actions.tasks_version();
    let approved = actions.approve(&fixture.carol, &writer, run.id, false).await.unwrap();
    assert_eq!(
        (approved.status, approved.approved_by, approved.trusted),
        (Status::Waiting, Some(fixture.carol.id()), false),
        "still read-only"
    );
    assert!(actions.tasks_version() > version);
    assert!(matches!(actions.approve(&fixture.carol, &writer, run.id, false).await, Err(ServiceError::Invalid(_))), "approved once");
    let audit = fixture.services.audit.recent_for(&fixture.carol, 10).await.unwrap();
    assert_eq!(audit.iter().filter(|event| event.action == AuditAction::ActionsRunApproved).count(), 1);

    // Re-running is not approving: the author is still nobody in particular.
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.database.actions_runs().cancel_run(repo, run.id, Timestamp::now()).await.unwrap();
    let again = actions.rerun(&fixture.carol, &writer, run.id, Rerun::All).await.unwrap();
    assert_eq!(again.status, Status::AwaitingApproval);

    let trusted = actions.approve(&fixture.alice, &fixture.repo(&fixture.alice).await, run.id, true).await.unwrap();
    assert_eq!(trusted.status, Status::Waiting);
    assert!(fixture.database.actions_settings().is_trusted(repo, fixture.bob.id()).await.unwrap());
    let audit = fixture.services.audit.recent_for(&fixture.alice, 10).await.unwrap();
    assert!(audit.iter().any(|event| event.action == AuditAction::ActionsUserTrusted));
    let next = opened(&fixture, issue, &fixture.bob).await;
    assert_eq!((next.status, next.trusted), (Status::Waiting, false), "trusted authors skip the wait, never the read-only token");
}

#[tokio::test]
async fn untrusted_runs_meet_their_concurrency_group_only_when_approved() {
    let fixture = Fixture::new().await;
    let grouped = "name: CI\non: pull_request\nconcurrency:\n  group: ci-${{ github.event.pull_request.number }}\n  cancel-in-progress: true\njobs:\n  a:\n    runs-on: x\n";
    let (issue, first) = fork_pull(&fixture, grouped).await;
    assert_eq!(first.concurrency_group.as_deref(), Some("pr-1/ci-1"), "namespaced under the pull request");
    let second = opened(&fixture, issue, &fixture.bob).await;
    let store = fixture.database.actions_runs();
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    assert_eq!(store.find_run(repo, first.id).await.unwrap().unwrap().status, Status::AwaitingApproval, "an awaiting run cancels nothing");

    // A writer's trusted run of the same pull request is in another group.
    let trusted = fixture
        .actions()
        .handle_pull_request(
            repo,
            issue,
            Some(fixture.carol.id()),
            &IssueAction::Synchronized { before: "0".repeat(40), after: "1".repeat(40), source: PushSource::User(fixture.carol.id()) },
        )
        .await
        .unwrap()
        .remove(0);
    assert_eq!((trusted.concurrency_group.as_deref(), trusted.status), (Some("ci-1"), Status::Waiting));

    let writer = fixture.repo(&fixture.carol).await;
    let approved = fixture.actions().approve(&fixture.carol, &writer, second.id, false).await.unwrap();
    assert_eq!(approved.status, Status::Waiting);
    assert_eq!(store.find_run(repo, first.id).await.unwrap().unwrap().status, Status::Cancelled, "approval applies cancel-in-progress");
    assert_eq!(store.find_run(repo, trusted.id).await.unwrap().unwrap().status, Status::Waiting, "never another group");
}

#[tokio::test]
async fn an_approved_run_waits_while_its_group_is_busy() {
    let fixture = Fixture::new().await;
    let queued = "name: CI\non: pull_request\nconcurrency: one-at-a-time\njobs:\n  a:\n    runs-on: x\n";
    let (issue, first) = fork_pull(&fixture, queued).await;
    let second = opened(&fixture, issue, &fixture.bob).await;
    let writer = fixture.repo(&fixture.carol).await;
    let actions = fixture.actions();
    assert_eq!(actions.approve(&fixture.carol, &writer, first.id, false).await.unwrap().status, Status::Waiting);
    assert_eq!(actions.approve(&fixture.carol, &writer, second.id, false).await.unwrap().status, Status::Blocked, "held for the group");
    let finished = actions.cancel(&fixture.carol, &writer, first.id).await.unwrap();
    assert_eq!(finished.status, Status::Cancelled);
    let repo = writer.repo.repo.id;
    assert_eq!(fixture.database.actions_runs().find_run(repo, second.id).await.unwrap().unwrap().status, Status::Waiting, "released");
}
