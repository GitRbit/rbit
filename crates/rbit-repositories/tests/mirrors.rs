use rbit_contracts::{RepoId, Timestamp};
use rbit_repositories::{
    mirrors::{MirrorPhase, MirrorSync, sync},
    push_mirrors::{MirrorPush, push},
};
use std::{
    future::{Future, ready},
    sync::Mutex,
};

/// Both ports; `trace` records calls in order.
#[derive(Default)]
struct World {
    trace: Mutex<Vec<String>>,
    missing_mirror: bool,
    missing_repository: bool,
    archived: bool,
    error: Option<&'static str>,
}
impl World {
    fn push(&self, entry: String) {
        self.trace.lock().unwrap().push(entry);
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
}

impl MirrorSync for World {
    type Error = &'static str;
    type Mirror = ();
    type Repository = ();
    fn mirror(&self, repo: RepoId) -> impl Future<Output = Result<Option<()>, &'static str>> + Send {
        self.push(format!("mirror:{}", repo.0));
        ready(Ok((!self.missing_mirror).then_some(())))
    }
    fn repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<()>, &'static str>> + Send {
        self.push(format!("repository:{}", repo.0));
        ready(Ok((!self.missing_repository).then_some(())))
    }
    fn is_archived(&self, (): &()) -> bool {
        self.archived
    }
    fn set_phase(&self, repo: RepoId, phase: Option<MirrorPhase>) {
        self.push(format!("phase:{}:{phase:?}", repo.0));
    }
    fn fetch(&self, (): &(), (): &()) -> impl Future<Output = Result<(), String>> + Send {
        self.push("fetch".into());
        ready(self.error.map_or(Ok(()), |error| Err(error.to_owned())))
    }
    fn observe(&self, (): &(), error: Option<&str>) {
        self.push(format!("observe:{error:?}"));
    }
    fn record(&self, repo: RepoId, at: Timestamp, error: Option<&str>) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.push(format!("record:{}:{}:{error:?}", repo.0, at.0));
        ready(Ok(()))
    }
}

#[tokio::test]
async fn a_fetch_shows_progress_records_its_outcome_and_always_clears_progress() {
    let world = World { error: Some("unreachable"), ..World::default() };
    sync(&world, RepoId(4), || Timestamp(9)).await.unwrap();
    assert_eq!(
        world.trace(),
        [
            "mirror:4",
            "repository:4",
            "phase:4:Some(Connecting)",
            "fetch",
            "observe:Some(\"unreachable\")",
            "record:4:9:Some(\"unreachable\")",
            "phase:4:None",
        ]
    );
}

#[tokio::test]
async fn gone_or_archived_mirrors_are_skipped_without_progress() {
    for (world, trace) in [
        (World { missing_mirror: true, ..World::default() }, vec!["mirror:4"]),
        (World { missing_repository: true, ..World::default() }, vec!["mirror:4", "repository:4"]),
        (World { archived: true, ..World::default() }, vec!["mirror:4", "repository:4"]),
    ] {
        sync(&world, RepoId(4), || panic!("no record, no clock")).await.unwrap();
        assert_eq!(world.trace(), trace);
    }
}

impl MirrorPush for World {
    type Error = &'static str;
    type Id = i64;
    type Target = ();
    type Repository = ();
    fn target(&self, id: i64) -> impl Future<Output = Result<Option<()>, &'static str>> + Send {
        self.push(format!("target:{id}"));
        ready(Ok((!self.missing_mirror).then_some(())))
    }
    fn claim(&self, id: i64, next: Timestamp) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.push(format!("claim:{id}:{}", next.0));
        ready(Ok(()))
    }
    fn repository(&self, (): &()) -> impl Future<Output = Result<Option<()>, &'static str>> + Send {
        self.push("repository".into());
        ready(Ok((!self.missing_repository).then_some(())))
    }
    fn is_archived(&self, (): &()) -> bool {
        self.archived
    }
    fn push(&self, (): &(), (): &()) -> impl Future<Output = Result<(), String>> + Send {
        self.push("push".into());
        ready(self.error.map_or(Ok(()), |error| Err(error.to_owned())))
    }
    fn observe(&self, (): &(), id: i64, error: Option<&str>) {
        self.push(format!("observe:{id}:{error:?}"));
    }
    fn record(&self, id: i64, at: Timestamp, error: Option<&str>) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.push(format!("record:{id}:{}:{error:?}", at.0));
        ready(Ok(()))
    }
}

#[tokio::test]
async fn a_push_is_claimed_an_interval_ahead_before_anything_else() {
    let world = World::default();
    let ticks = Mutex::new(100);
    let clock = || {
        let mut tick = ticks.lock().unwrap();
        *tick += 1;
        Timestamp(*tick)
    };
    push(&world, 7, 2, clock).await.unwrap();
    assert_eq!(world.trace(), ["target:7", "claim:7:221", "repository", "push", "observe:7:None", "record:7:102:None"]);

    let world = World { archived: true, ..World::default() };
    push(&world, 7, 2, || Timestamp(0)).await.unwrap();
    assert_eq!(world.trace(), ["target:7", "claim:7:120", "repository"], "archived: claimed, paused, not recorded");
    let world = World { missing_mirror: true, ..World::default() };
    push(&world, 7, 2, || panic!("no claim for a missing mirror")).await.unwrap();
    assert_eq!(world.trace(), ["target:7"]);
}
