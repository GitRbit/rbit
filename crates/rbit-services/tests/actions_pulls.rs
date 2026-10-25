//! Actions workstream T: pull request runs and their trust (§11.1) — who caused the
//! event, their access now, forks, vouched and suspended authors — closing, activity types
//! and skip markers, against real stores and repositories.

use rbit_core::{
    IssueId, PushSource, StateReason, Timestamp, User,
    actions::{RunEvent, RunTrust, Status},
    issue::IssueAction,
    pull::HeadSpec,
};
use rbit_services::{forks::ForkInput, pulls::NewPullInput};
use rbit_storage::sqlite::pulls::MergeRecord;
use serde_json::Value;

use crate::actions_fixture::{Fixture, git, workflow};

/// `alice/rocket` with pull request workflows on `main` and branches `feature` and
/// `topic`; `bob/rocket` is bob's fork with branches `fix` and `fix2`.
async fn fixture() -> Fixture {
    let fixture = Fixture::new().await;
    let types = "name: Activity\non:\n  pull_request:\n    types: [closed, edited, ready_for_review]\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    fixture.commit(&[(".forgejo/workflows/pr.yml", &workflow("PR", "pull_request")), (".forgejo/workflows/types.yml", types)], "CI");
    fixture.push_ref("refs/heads/main").await;
    fixture.commit(&[("feature.txt", "one\n")], "Feature");
    fixture.push_ref("refs/heads/feature").await;
    fixture.push_ref("refs/heads/topic").await;
    let parent = fixture.repo(&fixture.bob).await;
    fixture.services.forks.fork(&fixture.bob, &parent, ForkInput::default()).await.unwrap();
    let fork = fixture.services.repositories.open(Some(&fixture.bob), "bob", "rocket").await.unwrap();
    for branch in ["fix", "fix2"] {
        git(&fixture.work, &["push", "-q", fork.disk_path.to_str().unwrap(), &format!("HEAD:refs/heads/{branch}")]);
    }
    fixture
}

/// Opens a pull request of `head` (`owner:branch`) by `author`; returns its issue.
async fn open(fixture: &Fixture, author: &User, head: &str, title: &str) -> IssueId {
    let base = fixture.repo(author).await;
    let spec = HeadSpec::parse(head).unwrap();
    let head_repo = fixture.services.pulls.resolve_head(Some(author), &base, &spec).await.unwrap();
    let branch = head.split(':').nth(1).unwrap().to_owned();
    let input = NewPullInput { base_branch: "main".into(), head_branch: branch, title: title.into(), ..NewPullInput::default() };
    fixture.services.pulls.create(author, &base, &head_repo, input).await.unwrap().0.id
}

async fn event(fixture: &Fixture, issue: IssueId, actor: &User, action: IssueAction) -> Vec<rbit_core::actions::Run> {
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_pull_request(repo, issue, Some(actor.id()), &action).await.unwrap()
}

fn synchronize(source: PushSource) -> IssueAction {
    IssueAction::Synchronized { before: "0".repeat(40), after: "1".repeat(40), source }
}

#[tokio::test]
async fn writers_pull_requests_are_trusted_and_carry_the_pull_request_event() {
    let fixture = Box::pin(fixture()).await;
    let issue = open(&fixture, &fixture.carol, "alice:feature", "Add the feature").await;
    let runs = event(&fixture, issue, &fixture.carol, IssueAction::Opened).await;
    assert_eq!(runs.len(), 1, "{runs:?}");
    let run = &runs[0];
    assert_eq!((run.event, run.reference.as_str(), run.pull_number), (RunEvent::PullRequest, "refs/pull/1/head", Some(1)));
    assert_eq!((run.trusted, run.is_fork, run.status), (true, false, Status::Waiting));
    assert_eq!((run.title.as_str(), run.actor), ("Add the feature", Some(fixture.carol.id())));
    let (payload, _) = fixture.database.actions_runs().event(run.id).await.unwrap().unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!((payload["action"].as_str(), payload["number"].as_i64()), (Some("opened"), Some(1)));
    assert_eq!(
        (payload["pull_request"]["head"]["ref"].as_str(), payload["pull_request"]["base"]["ref"].as_str()),
        (Some("feature"), Some("main"))
    );
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    let context = fixture.actions().task_context(run, &jobs[0]).await.unwrap();
    assert_eq!((context["head_ref"].as_str(), context["base_ref"].as_str()), (Some("feature"), Some("main")));
}

#[tokio::test]
async fn readers_and_forks_wait_for_approval_with_a_read_only_run() {
    let fixture = Box::pin(fixture()).await;
    let same_repo = open(&fixture, &fixture.rita, "alice:feature", "From a reader").await;
    let runs = event(&fixture, same_repo, &fixture.rita, IssueAction::Opened).await;
    assert_eq!(
        (runs[0].trusted, runs[0].is_fork, runs[0].status),
        (false, false, Status::AwaitingApproval),
        "opening needs only read access"
    );
    let jobs = fixture.database.actions_runs().jobs(runs[0].id).await.unwrap();
    assert!(jobs.iter().all(|job| job.status == Status::AwaitingApproval));

    let fork = open(&fixture, &fixture.bob, "bob:fix", "From a fork").await;
    let runs = event(&fixture, fork, &fixture.bob, IssueAction::Opened).await;
    assert_eq!((runs[0].trusted, runs[0].is_fork, runs[0].status), (false, true, Status::AwaitingApproval));
    // A maintainer reopening it does not make the author's code trusted.
    let runs = event(&fixture, fork, &fixture.alice, IssueAction::Reopened).await;
    assert_eq!((runs[0].trusted, runs[0].status), (false, Status::AwaitingApproval));
    assert_eq!((runs[0].actor, runs[0].trigger_actor), (Some(fixture.bob.id()), Some(fixture.alice.id())));
}

#[tokio::test]
async fn synchronize_is_judged_by_whoever_pushed_the_new_head() {
    let fixture = Box::pin(fixture()).await;
    let fork = open(&fixture, &fixture.bob, "bob:fix", "From a fork").await;
    let runs = event(&fixture, fork, &fixture.carol, synchronize(PushSource::User(fixture.carol.id()))).await;
    assert_eq!((runs[0].trusted, runs[0].status), (true, Status::Waiting), "a writer pushed the head");
    let runs = event(&fixture, fork, &fixture.bob, synchronize(PushSource::User(fixture.bob.id()))).await;
    assert_eq!((runs[0].trusted, runs[0].status), (false, Status::AwaitingApproval));
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let deploy_key = fixture.actions().handle_pull_request(repo, fork, None, &synchronize(PushSource::DeployKey)).await.unwrap();
    assert_eq!((deploy_key[0].trusted, deploy_key[0].status), (false, Status::AwaitingApproval), "an unknown pusher is untrusted");
    for source in [PushSource::Mirror, PushSource::Import, PushSource::Task(rbit_core::actions::TaskId(4)), PushSource::System] {
        assert!(fixture.actions().handle_pull_request(repo, fork, None, &synchronize(source)).await.unwrap().is_empty(), "{source:?}");
    }
}

#[tokio::test]
async fn vouched_authors_skip_the_wait_but_never_get_secrets() {
    let fixture = Box::pin(fixture()).await;
    let fork = open(&fixture, &fixture.bob, "bob:fix", "From a fork").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let settings = fixture.database.actions_settings();
    settings.trust(repo, fixture.bob.id(), fixture.alice.id(), Timestamp::now()).await.unwrap();
    let runs = event(&fixture, fork, &fixture.bob, IssueAction::Opened).await;
    assert_eq!((runs[0].trusted, runs[0].status), (false, Status::Waiting), "no approval needed, still read-only");
    // Someone else's push to a vouched author's pull request still waits.
    let runs = event(&fixture, fork, &fixture.rita, synchronize(PushSource::User(fixture.rita.id()))).await;
    assert_eq!(runs[0].status, Status::AwaitingApproval);

    fixture.database.admin().set_suspended(fixture.bob.id(), true).await.unwrap();
    let runs = event(&fixture, fork, &fixture.bob, IssueAction::Opened).await;
    assert_eq!(runs[0].status, Status::AwaitingApproval, "a suspended author always waits");
}

#[tokio::test]
async fn a_merged_pull_request_vouches_for_its_author() {
    let fixture = Box::pin(fixture()).await;
    let earlier = open(&fixture, &fixture.bob, "bob:fix", "Earlier").await;
    let head = fixture.database.pulls().find(earlier).await.unwrap().unwrap().head_sha;
    let merge =
        MergeRecord { merged_by: Some(fixture.alice.id()), strategy: None, commit: &head, base: &head, head: &head, at: Timestamp::now() };
    fixture.database.pulls().record_merge(earlier, &merge).await.unwrap();
    let later = open(&fixture, &fixture.bob, "bob:fix2", "Later").await;
    let runs = event(&fixture, later, &fixture.bob, IssueAction::Opened).await;
    assert_eq!((runs[0].trusted, runs[0].status), (false, Status::Waiting));
}

/// Every combination of who caused the event, their access now, whether the author is
/// vouched for and whether they are suspended.
#[tokio::test]
async fn the_trust_table() {
    let fixture = Box::pin(fixture()).await;
    let owned = fixture.repo(&fixture.alice).await.repo;
    let repo = owned.repo.id;
    let settings = fixture.database.actions_settings();
    let (alice, carol, rita, bob) = (&fixture.alice, &fixture.carol, &fixture.rita, &fixture.bob);
    // Access now: alice admin, carol write, rita read, bob none (a public repository).
    let users = [(alice, true), (carol, true), (rita, false), (bob, false)];
    for (author, author_writes) in users {
        for (other, other_writes) in users.iter().copied().filter(|(user, _)| user.id() != author.id()).take(2) {
            for vouched in [false, true] {
                for suspended in [false, true] {
                    if vouched {
                        settings.trust(repo, author.id(), alice.id(), Timestamp::now()).await.unwrap();
                    } else {
                        settings.untrust(repo, author.id()).await.unwrap();
                    }
                    fixture.database.admin().set_suspended(author.id(), suspended).await.unwrap();
                    let cases = [
                        ("author", Some(author.id()), author_writes && !suspended, !suspended && vouched),
                        ("other", Some(other.id()), other_writes, false),
                        ("unknown", None, false, false),
                    ];
                    for (who, actor, trusted, skips_wait) in cases {
                        let expected = if trusted { RunTrust::Trusted } else { RunTrust::Untrusted { awaits_approval: !skips_wait } };
                        let got = fixture.actions().classify_pull_request(&owned, actor, Some(author.id())).await.unwrap();
                        assert_eq!(
                            got,
                            expected,
                            "author {} (writes: {author_writes}), actor {who}, vouched {vouched}, suspended {suspended}",
                            author.name()
                        );
                    }
                }
            }
        }
        fixture.database.admin().set_suspended(author.id(), false).await.unwrap();
    }
    let gone = fixture.actions().classify_pull_request(&owned, None, None).await.unwrap();
    assert_eq!(gone, RunTrust::Untrusted { awaits_approval: true }, "a deleted author is nobody");
}

#[tokio::test]
async fn closing_cancels_runs_that_have_not_started_and_runs_closed_workflows() {
    let fixture = Box::pin(fixture()).await;
    let fork = open(&fixture, &fixture.bob, "bob:fix", "From a fork").await;
    let awaiting = event(&fixture, fork, &fixture.bob, IssueAction::Opened).await.remove(0);
    let trusted = event(&fixture, fork, &fixture.carol, synchronize(PushSource::User(fixture.carol.id()))).await.remove(0);
    let closed = event(&fixture, fork, &fixture.alice, IssueAction::Closed { reason: StateReason::Completed }).await;
    let names: Vec<_> = closed.iter().map(|run| run.workflow_name.as_str()).collect();
    assert_eq!(names, ["Activity"], "only workflows listing `closed`");
    assert_eq!(closed[0].status, Status::AwaitingApproval, "judged by the author, not the maintainer who closed it");
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let store = fixture.database.actions_runs();
    assert_eq!(store.find_run(repo, awaiting.id).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(store.find_run(repo, trusted.id).await.unwrap().unwrap().status, Status::Cancelled);
}

#[tokio::test]
async fn activity_types_and_skip_markers() {
    let fixture = Box::pin(fixture()).await;
    let issue = open(&fixture, &fixture.carol, "alice:topic", "Topic").await;
    let edited = IssueAction::Edited { previous_title: Some("Old".into()), previous_body: None, previous_base: None };
    let runs = event(&fixture, issue, &fixture.carol, edited).await;
    assert_eq!(runs.iter().map(|run| run.workflow_name.as_str()).collect::<Vec<_>>(), ["Activity"]);
    let runs = event(&fixture, issue, &fixture.carol, IssueAction::ReadyForReview).await;
    assert_eq!(runs.iter().map(|run| run.workflow_name.as_str()).collect::<Vec<_>>(), ["Activity"]);
    assert!(event(&fixture, issue, &fixture.carol, IssueAction::Locked).await.is_empty(), "rbit runs nothing on other actions");

    let skipped = open(&fixture, &fixture.carol, "alice:feature", "Docs [skip ci]").await;
    assert!(event(&fixture, skipped, &fixture.carol, IssueAction::Opened).await.is_empty(), "a skip marker in the title");
}
