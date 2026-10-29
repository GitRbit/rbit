//! Actions workstream T: jobs report commit statuses as rbit Actions — on queue, start and
//! finish, never twice the same — and scheduled runs report none.

use rbit_core::{
    PushSource, Timestamp,
    actions::Status,
    status::{CommitStatus, StatusSource, StatusState},
};
use rbit_services::DomainEvent;

use crate::actions_fixture::Fixture;

const CHAIN: &str = "name: CI\non: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n  b:\n    needs: a\n    runs-on: x\n    steps: [{run: x}]\n  c:\n    runs-on: y\n    steps: [{run: x}]\n";

async fn latest(fixture: &Fixture, sha: &str) -> Vec<(String, StatusState, String)> {
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let statuses = fixture.database.commit_statuses().latest(repo, sha).await.unwrap();
    statuses.into_iter().map(|status| (status.context.as_str().to_owned(), status.state, status.description.unwrap_or_default())).collect()
}

fn rows(rows: &[(&str, StatusState, &str)]) -> Vec<(String, StatusState, String)> {
    rows.iter().map(|(context, state, description)| ((*context).to_owned(), *state, (*description).to_owned())).collect()
}

#[tokio::test]
async fn jobs_report_their_state_as_rbit_actions() {
    let fixture = Fixture::new().await;
    let mut events = fixture.services.events.subscribe();
    fixture.commit(&[(".forgejo/workflows/ci.yml", CHAIN)], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let actions = fixture.actions();
    let run = actions.handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    assert_eq!(
        latest(&fixture, &run.sha).await,
        rows(&[
            ("CI / a (push)", StatusState::Pending, "Queued"),
            ("CI / b (push)", StatusState::Pending, "Queued"),
            ("CI / c (push)", StatusState::Pending, "Queued"),
        ])
    );
    let all: Vec<CommitStatus> = fixture.database.commit_statuses().list(repo, &run.sha, 0, 100).await.unwrap();
    assert_eq!(all.len(), 3);
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    let a = all.iter().find(|status| status.context.as_str() == "CI / a (push)").unwrap();
    assert_eq!((a.creator, a.source), (None, StatusSource::Actions));
    assert_eq!(
        a.target_url.clone().unwrap(),
        format!("https://git.example.com/alice/rocket/actions/runs/{}/jobs/{}", run.number, jobs[0].id.0)
    );
    let published = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| matches!(event, DomainEvent::CommitStatus { actor: None, .. }))
        .count();
    assert_eq!(published, 3, "each status is announced, by nobody");

    let claimed = fixture.claim(&["x"], 1).await.unwrap();
    actions.report_statuses(run.id).await.unwrap();
    actions.report_statuses(run.id).await.unwrap();
    assert_eq!(latest(&fixture, &run.sha).await[0], ("CI / a (push)".to_owned(), StatusState::Pending, "In progress".to_owned()));
    assert_eq!(fixture.database.commit_statuses().count(repo, &run.sha).await.unwrap(), 4, "a report is not repeated");

    let store = fixture.database.actions_runs();
    let settled = store.finish_task(claimed.task.id, Status::Success, Timestamp::now()).await.unwrap().unwrap();
    actions.settled(&settled).await.unwrap();
    let now = latest(&fixture, &run.sha).await;
    assert_eq!((now[0].1, now[0].2.starts_with("Successful in ")), (StatusState::Success, true), "{now:?}");
    assert_eq!(now[1], ("CI / b (push)".to_owned(), StatusState::Pending, "Queued".to_owned()), "unblocked, still queued");

    let writer = fixture.repo(&fixture.carol).await;
    actions.cancel(&fixture.carol, &writer, run.id).await.unwrap();
    let now = latest(&fixture, &run.sha).await;
    assert_eq!((now[1].1, now[1].2.as_str()), (StatusState::Error, "Cancelled"));
    assert_eq!((now[2].1, now[2].2.as_str()), (StatusState::Error, "Cancelled"));
    assert_eq!(now[0].1, StatusState::Success, "finished jobs keep theirs");
    let combined = fixture.services.statuses.combined_for(&writer, &run.sha).await.unwrap();
    assert_eq!(combined.state, StatusState::Failure, "a cancelled job fails a required check");
}

#[tokio::test]
async fn awaiting_runs_say_so_and_scheduled_runs_report_nothing() {
    let fixture = Fixture::new().await;
    let nightly = "name: Nightly\non:\n  schedule:\n    - cron: '0 3 * * *'\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    fixture.commit(&[(".forgejo/workflows/nightly.yml", nightly)], "Nightly");
    fixture.push_ref("refs/heads/main").await;
    let owned = fixture.repo(&fixture.alice).await.repo;
    let head = fixture.repo(&fixture.alice).await.reader.commit_id("main".into()).await.unwrap();
    let actions = fixture.actions();
    actions.refresh_schedules(&owned, &head, Timestamp(0)).await.unwrap();
    let runs = actions.tick(Timestamp(4 * 3600)).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert!(latest(&fixture, &head).await.is_empty(), "no commit is being judged");

    // A reader's pull request waits for approval and says so on its head.
    let pr = "name: PR\non: pull_request\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    fixture.commit(&[(".forgejo/workflows/pr.yml", pr)], "PR");
    fixture.push_ref("refs/heads/feature").await;
    let base = fixture.repo(&fixture.rita).await;
    let input = rbit_services::pulls::NewPullInput { base_branch: "main".into(), head_branch: "feature".into(), ..Default::default() };
    let issue = fixture.services.pulls.create(&fixture.rita, &base, &base, input).await.unwrap().0.id;
    let run = actions
        .handle_pull_request(owned.repo.id, issue, Some(fixture.rita.id()), &rbit_core::issue::IssueAction::Opened)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(run.status, Status::AwaitingApproval);
    assert_eq!(latest(&fixture, &run.sha).await, rows(&[("PR / a (pull_request)", StatusState::Pending, "Waiting for approval")]));
    actions.approve(&fixture.carol, &fixture.repo(&fixture.carol).await, run.id, false).await.unwrap();
    assert_eq!(latest(&fixture, &run.sha).await, rows(&[("PR / a (pull_request)", StatusState::Pending, "Queued")]));
}
