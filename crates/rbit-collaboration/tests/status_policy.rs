#![cfg(feature = "applications")]
//! Status submission order, authority and error text without concrete persistence.

use std::sync::Mutex;

use rbit_collaboration::statuses::{
    CommitStatus, CommitStatusRepository, StatusAccess, StatusApplication, StatusApplicationError, StatusAuthor, StatusContext,
    StatusError, StatusId, StatusSource, StatusState, creator_ids,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};

const SHA: &str = "ABCDEF0123456789abcdef0123456789abcdef01";

#[derive(Default)]
struct Statuses {
    calls: Mutex<Vec<String>>,
    full: bool,
    stored: Vec<CommitStatus>,
}

fn status(id: i64, sha: &str, context: &str, state: StatusState, creator: Option<i64>) -> CommitStatus {
    CommitStatus {
        id: StatusId(id),
        repo_id: RepoId(3),
        sha: sha.to_owned(),
        state,
        context: StatusContext::parse(context).unwrap(),
        description: None,
        target_url: None,
        creator: creator.map(AccountId),
        source: if creator.is_some() { StatusSource::User } else { StatusSource::Actions },
        created_at: Timestamp(id),
        updated_at: Timestamp(id),
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl CommitStatusRepository for Statuses {
    type Error = &'static str;
    type Report = &'static str;

    async fn create(
        &self,
        repo: RepoId,
        sha: &str,
        report: &&'static str,
        author: StatusAuthor,
        cap: u32,
    ) -> Result<Option<CommitStatus>, Self::Error> {
        self.calls.lock().unwrap().push(format!("create {} {sha} {report} {author:?} {cap}", repo.0));
        let creator = match author {
            StatusAuthor::Account(id) => Some(id.0),
            StatusAuthor::Actions => None,
        };
        Ok((!self.full).then(|| status(9, sha, "ci", StatusState::Success, creator)))
    }

    async fn find(&self, _: RepoId, id: StatusId) -> Result<Option<CommitStatus>, Self::Error> {
        Ok(self.stored.iter().find(|status| status.id == id).cloned())
    }

    async fn page(&self, _: RepoId, _: &str, offset: u64, limit: u32) -> Result<(Vec<CommitStatus>, u64), Self::Error> {
        self.calls.lock().unwrap().push(format!("page {offset} {limit}"));
        Ok((self.stored.clone(), 40))
    }

    async fn latest(&self, _: RepoId, sha: &str) -> Result<Vec<CommitStatus>, Self::Error> {
        Ok(self.stored.iter().filter(|status| status.sha == sha).cloned().collect())
    }

    async fn latest_of(&self, _: RepoId, _: &[String]) -> Result<Vec<CommitStatus>, Self::Error> {
        Ok(self.stored.clone())
    }
}

fn access(can_write: bool, archived: bool) -> StatusAccess {
    StatusAccess { repo: RepoId(3), can_write, archived }
}

type Outcome = Result<CommitStatus, StatusApplicationError<&'static str, &'static str>>;

async fn post(app: &StatusApplication<&Statuses>, sha: &str, report: Result<&'static str, StatusError>, commit: Option<&str>) -> Outcome {
    let commit = commit.map(str::to_owned);
    app.create(AccountId(5), sha, || report, |_| async move { Ok(commit) }).await
}

fn invalid(outcome: Outcome) -> String {
    match outcome {
        Err(StatusApplicationError::Invalid(message)) => message,
        other => panic!("expected an invalid submission, got {other:?}"),
    }
}

#[tokio::test]
async fn refusals_follow_the_legacy_order_and_store_nothing() {
    let store = Statuses::default();
    let full = SHA.to_ascii_lowercase();
    let bad = Err(StatusError::State);
    // Authority first, then archival, then the id, the values and the commit.
    assert!(matches!(
        post(&StatusApplication::new(&store, access(false, true)), "x", bad.clone(), None).await,
        Err(StatusApplicationError::Forbidden)
    ));
    assert!(matches!(
        post(&StatusApplication::new(&store, access(true, true)), "x", bad.clone(), None).await,
        Err(StatusApplicationError::Archived)
    ));
    let app = StatusApplication::new(&store, access(true, false));
    assert_eq!(invalid(post(&app, "main", bad.clone(), None).await), "sha must be a full commit id");
    assert_eq!(invalid(post(&app, SHA, bad, None).await), "state must be one of pending, success, failure or error");
    assert_eq!(invalid(post(&app, SHA, Ok("r"), None).await), format!("no commit found for SHA {full}"));
    assert_eq!(invalid(post(&app, SHA, Ok("r"), Some("0".repeat(40).as_str())).await), format!("no commit found for SHA {full}"));
    let failed = app.create(AccountId(5), SHA, || Ok("r"), |_| async { Err::<Option<String>, _>("git") }).await;
    assert!(matches!(failed, Err(StatusApplicationError::Commit("git"))));
    assert!(store.calls.lock().unwrap().is_empty(), "refused submissions reach no store");
}

#[tokio::test]
async fn accepted_submissions_are_capped_by_the_store() {
    let store = Statuses::default();
    let full = SHA.to_ascii_lowercase();
    let app = StatusApplication::new(&store, access(true, false));
    let status = post(&app, SHA, Ok("r"), Some(&full)).await.unwrap();
    assert_eq!((status.sha.as_str(), status.creator), (full.as_str(), Some(AccountId(5))));
    assert_eq!(store.calls.lock().unwrap().as_slice(), [format!("create 3 {full} r Account(AccountId(5)) 1000")]);

    let capped = Statuses { full: true, ..Statuses::default() };
    let app = StatusApplication::new(&capped, access(true, false));
    assert_eq!(
        invalid(post(&app, SHA, Ok("r"), Some(&full)).await),
        "this SHA and context has reached the maximum number of statuses (1000)"
    );
    assert_eq!(app.record_system(&full, &"r").await, Ok(None), "Actions statuses at the cap are dropped");
}

#[tokio::test]
async fn task_tokens_post_only_to_their_own_repository_as_actions() {
    let store = Statuses::default();
    let full = SHA.to_ascii_lowercase();
    let app = StatusApplication::new(&store, access(true, false));
    let other = app.create_for_task(RepoId(4), "x", || Err(StatusError::State), |_| async { Ok::<_, &str>(None) }).await;
    assert!(matches!(other, Err(StatusApplicationError::Forbidden)), "another repository's token is refused first");
    let commit = full.clone();
    let status = app.create_for_task(RepoId(3), SHA, || Ok("r"), |_| async move { Ok::<_, &str>(Some(commit)) }).await.unwrap();
    assert_eq!((status.creator, status.source), (None, StatusSource::Actions));
    let read_only = StatusApplication::new(&store, access(false, false));
    let refused = read_only.create_for_task(RepoId(3), SHA, || Ok("r"), |_| async { Ok::<_, &str>(None) }).await;
    assert!(matches!(refused, Err(StatusApplicationError::Forbidden)), "a task needs write access like a person");
}

#[tokio::test]
async fn reads_combine_the_latest_status_of_each_context() {
    let (a, b) = ("a".repeat(40), "b".repeat(40));
    let stored = vec![
        status(1, &a, "build", StatusState::Success, Some(7)),
        status(2, &a, "lint", StatusState::Error, Some(5)),
        status(3, &b, "build", StatusState::Pending, Some(7)),
        status(4, &b, "deploy", StatusState::Success, None),
    ];
    let store = Statuses { stored, ..Statuses::default() };
    let app = StatusApplication::new(&store, access(false, false));
    assert_eq!(app.combined(a.clone()).await.unwrap().state, StatusState::Failure);
    assert_eq!(app.combined("c".repeat(40)).await.unwrap().state, StatusState::Pending, "nothing reported");
    let by_commit = app.combined_of(&[a.clone(), b.clone()]).await.unwrap();
    assert_eq!((by_commit[&a].statuses.len(), by_commit[&b].state), (2, StatusState::Pending));
    let page = app.page(a.clone(), 20, 10).await.unwrap();
    assert_eq!((page.sha, page.total), (a, 40));
    assert_eq!(app.find(StatusId(4)).await.unwrap().map(|status| status.creator), Some(None));
    assert_eq!(creator_ids(&store.stored), [AccountId(5), AccountId(7)], "once each, Actions excluded");
}
