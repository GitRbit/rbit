//! Pull request trust over scripted facts: fork override, standing order and short-circuits.
use rbit_access::RepoAccess;
use rbit_actions::trust::{ActorFacts, PullRequestTrust, RunTrust, TrustFacts};
use std::{
    collections::BTreeMap,
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

/// Accounts by id; every read is logged.
#[derive(Default)]
struct Facts {
    actors: BTreeMap<u8, ActorFacts>,
    suspended: Vec<u8>,
    trusted: Vec<u8>,
    merged: Vec<u8>,
    merged_vouches: bool,
    failing: Option<&'static str>,
    reads: Mutex<Vec<String>>,
}

impl Facts {
    fn read(&self, what: &'static str, id: Option<u8>) -> Result<(), &'static str> {
        self.reads.lock().unwrap().push(id.map_or_else(|| what.to_owned(), |id| format!("{what} {id}")));
        if self.failing == Some(what) { Err(what) } else { Ok(()) }
    }

    fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }
}

impl TrustFacts for Facts {
    type Account = u8;
    type Error = &'static str;

    fn actor(&self, id: u8) -> impl Future<Output = Result<Option<ActorFacts>, &'static str>> + Send {
        ready(self.read("actor", Some(id)).map(|()| self.actors.get(&id).copied()))
    }
    fn is_suspended(&self, id: u8) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(self.read("suspended", Some(id)).map(|()| self.suspended.contains(&id)))
    }
    fn is_trusted_user(&self, id: u8) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(self.read("trusted", Some(id)).map(|()| self.trusted.contains(&id)))
    }
    fn merged_pull_vouches(&self) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(self.read("policy", None).map(|()| self.merged_vouches))
    }
    fn has_merged_pull(&self, id: u8) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(self.read("merged", Some(id)).map(|()| self.merged.contains(&id)))
    }
}

const WRITER: u8 = 1;
const READER: u8 = 2;
const WAITS: RunTrust = RunTrust::Untrusted { awaits_approval: true };
const RUNS: RunTrust = RunTrust::Untrusted { awaits_approval: false };

fn base() -> Facts {
    let actors = BTreeMap::from([
        (WRITER, ActorFacts { access: RepoAccess::Write, suspended: false }),
        (READER, ActorFacts { access: RepoAccess::Read, suspended: false }),
    ]);
    Facts { actors, merged_vouches: true, ..Facts::default() }
}

#[test]
fn a_fork_is_never_trusted_even_when_a_writer_pushed_it() {
    let facts = base();
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(WRITER), Some(READER), false)), Ok(RunTrust::Trusted));
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(WRITER), Some(READER), true)), Ok(RUNS), "the writer only skips the wait");
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), true)), Ok(WAITS));
}

#[test]
fn standing_reads_stop_at_the_first_deciding_fact() {
    let facts = Facts { suspended: vec![READER], trusted: vec![READER], ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), true)), Ok(WAITS));
    assert_eq!(facts.reads(), ["actor 2", "suspended 2"]);

    let facts = Facts { trusted: vec![READER], merged: vec![READER], ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), true)), Ok(RUNS));
    assert_eq!(facts.reads(), ["actor 2", "suspended 2", "trusted 2"]);

    let facts = Facts { merged: vec![READER], ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), true)), Ok(RUNS));
    assert_eq!(facts.reads(), ["actor 2", "suspended 2", "trusted 2", "policy", "merged 2"]);

    let facts = Facts { merged: vec![READER], merged_vouches: false, ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), true)), Ok(WAITS), "all_external_contributors");
    assert_eq!(facts.reads(), ["actor 2", "suspended 2", "trusted 2", "policy"]);
}

#[test]
fn unknown_and_gone_accounts_wait_and_someone_else_never_vouches() {
    let facts = Facts { trusted: vec![READER], ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, None, Some(READER), false)), Ok(WAITS), "deploy key push");
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(9), Some(READER), false)), Ok(WAITS), "gone pusher");
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), None, false)), Ok(WAITS), "gone author");
    let facts = Facts { trusted: vec![9], ..base() };
    assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(9), false)), Ok(WAITS), "vouched author, other pusher");
}

#[test]
fn errors_propagate_from_the_fact_that_failed() {
    for failing in ["actor", "suspended", "trusted", "policy", "merged"] {
        let facts = Facts { failing: Some(failing), ..base() };
        assert_eq!(run(PullRequestTrust::classify(&facts, Some(READER), Some(READER), false)), Err(failing));
    }
}
