//! Artifact store decisions over a scripted, immediately ready port.
// Scripted ports answer at once; the traits still return futures.
#![allow(clippy::unused_async_trait_impl, clippy::struct_excessive_bools)]
use rbit_actions::{
    archives::{ArchiveFiles, ArchiveUploads, Received, Refusal, StoreError},
    artifacts::{
        ArtifactLimits, ArtifactRecorded, ArtifactRecords, ArtifactRetention, ArtifactStorage, ArtifactUpload, ArtifactUploadStore,
        NewArtifact,
    },
};
use std::{
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("scripted ports are always ready"),
    }
}

const DIGEST: [u8; 32] = [0xab; 32];
const HEX: &str = "abababababababababababababababababababababababababababababababab";
const LIMITS: ArtifactLimits = ArtifactLimits { max_artifact_bytes: 100, max_storage_bytes: 1_000, retention_days: 90 };

#[derive(Debug, Default)]
struct Store {
    calls: Mutex<Vec<String>>,
    stored: u64,
    digest: Option<[u8; 32]>,
    lease_lost: bool,
    exists: bool,
    replaces: bool,
    place_fails: bool,
    recorded: Mutex<Option<(u64, i64)>>,
}

impl Store {
    fn call(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ArchiveFiles for Store {
    type Error = &'static str;
    type Location = String;
    async fn remove(&self, location: &String) {
        self.call(format!("remove {location}"));
    }
}

impl ArchiveUploads for Store {
    type Body = u64;
    type Temporary = &'static str;
    async fn receive(&self, body: u64, limit: u64) -> Result<Option<Received<&'static str>>, &'static str> {
        self.call(format!("receive {limit}"));
        Ok((body <= limit).then(|| Received { temporary: "tmp", size: body, sha256: self.digest.unwrap_or(DIGEST) }))
    }
    async fn discard(&self, temporary: &&'static str) {
        self.call(format!("discard {temporary}"));
    }
    async fn place(&self, _: &&'static str, location: &String) -> Result<(), &'static str> {
        self.call(format!("place {location}"));
        if self.place_fails { Err("disk") } else { Ok(()) }
    }
}

impl ArtifactRecords for Store {
    type Artifact = i64;
    fn location(&self, artifact: &i64) -> String {
        format!("file{artifact}")
    }
    async fn delete(&self, artifact: &i64) -> Result<(), &'static str> {
        self.call(format!("delete {artifact}"));
        Ok(())
    }
}

impl ArtifactUploadStore for Store {
    type Target = i64;
    async fn lease(&self) -> Result<i64, &'static str> {
        self.call("lease");
        Ok(7)
    }
    async fn still_leased(&self, _: &i64) -> Result<(), &'static str> {
        self.call("still leased");
        if self.lease_lost { Err("rerun") } else { Ok(()) }
    }
    async fn stored_bytes(&self, _: &i64) -> Result<u64, &'static str> {
        Ok(self.stored)
    }
    fn now(&self) -> i64 {
        1_000
    }
    async fn record(&self, new: &NewArtifact<'_, i64>, overwrite: bool) -> Result<ArtifactRecorded<i64>, &'static str> {
        self.call(format!("record {} {overwrite}", new.name));
        *self.recorded.lock().unwrap() = Some((new.size, new.expires_at));
        Ok(if self.exists {
            ArtifactRecorded::Exists
        } else {
            ArtifactRecorded::Stored { artifact: 2, replaced: self.replaces.then_some(1) }
        })
    }
}

impl ArtifactRetention for Store {
    async fn expired(&self, now: i64, limit: u32) -> Result<Vec<i64>, &'static str> {
        self.call(format!("expired {now} {limit}"));
        Ok(vec![4, 5])
    }
}

fn upload(store: &Store, name: &str, sha256: &str, days: Option<u32>, body: u64) -> Result<i64, StoreError<&'static str>> {
    run(ArtifactStorage::upload(store, &LIMITS, &ArtifactUpload { name, sha256, retention_days: days, overwrite: false }, body))
}

fn refusal(result: Result<i64, StoreError<&'static str>>) -> Refusal {
    match result {
        Err(StoreError::Refused(refusal)) => refusal,
        other => panic!("{other:?}"),
    }
}

#[test]
fn malformed_uploads_are_refused_before_the_lease() {
    let store = Store::default();
    let name = "artifact names are 1 to 128 letters, digits, '.', '_' or '-', and not '.' or '..'";
    assert_eq!(refusal(upload(&store, "..", HEX, None, 1)), Refusal::BadRequest(name.into()));
    assert_eq!(refusal(upload(&store, "a", "zz", None, 1)), Refusal::BadRequest("the content digest must be 64 hex digits".into()));
    assert_eq!(store.calls(), Vec::<String>::new());
}

#[test]
fn room_is_the_smaller_of_the_instance_cap_and_the_repository_quota() {
    let full = Store { stored: 1_000, ..Store::default() };
    assert_eq!(refusal(upload(&full, "a", HEX, None, 1)), Refusal::TooLarge("the repository's artifact storage is full".into()));
    assert_eq!(full.calls(), ["lease"]);
    let nearly = Store { stored: 950, ..Store::default() };
    let text = "the artifact does not fit in the repository's artifact storage";
    assert_eq!(refusal(upload(&nearly, "a", HEX, None, 51)), Refusal::TooLarge(text.into()));
    assert_eq!(nearly.calls(), ["lease", "receive 50"]);
    let instance = Store::default();
    assert_eq!(
        refusal(upload(&instance, "a", HEX, None, 101)),
        Refusal::TooLarge("the artifact is larger than this instance allows".into())
    );
}

#[test]
fn a_wrong_digest_or_a_lost_lease_discards_the_body() {
    let wrong = Store { digest: Some([0; 32]), ..Store::default() };
    assert_eq!(refusal(upload(&wrong, "a", HEX, None, 5)), Refusal::BadRequest("the artifact does not match its content digest".into()));
    assert_eq!(wrong.calls(), ["lease", "receive 100", "discard tmp"]);
    let lost = Store { lease_lost: true, ..Store::default() };
    assert!(matches!(upload(&lost, "a", HEX, None, 5), Err(StoreError::Port("rerun"))));
    assert_eq!(lost.calls(), ["lease", "receive 100", "still leased", "discard tmp"]);
}

#[test]
fn names_are_unique_per_attempt_unless_overwritten() {
    let exists = Store { exists: true, ..Store::default() };
    let text = "this run already has an artifact `a`; set `overwrite: true` to replace it";
    assert_eq!(refusal(upload(&exists, "a", HEX, None, 5)), Refusal::Conflict(text.into()));
    assert_eq!(exists.calls().last().unwrap(), "discard tmp");
    let replacing = Store { replaces: true, ..Store::default() };
    assert_eq!(upload(&replacing, "a", HEX, None, 5).unwrap(), 2);
    assert_eq!(replacing.calls(), ["lease", "receive 100", "still leased", "record a false", "remove file1", "place file2"]);
}

#[test]
fn a_file_that_cannot_be_placed_takes_its_row_with_it() {
    let store = Store { place_fails: true, ..Store::default() };
    assert!(matches!(upload(&store, "a", HEX, None, 5), Err(StoreError::Port("disk"))));
    assert_eq!(store.calls()[4..], ["place file2", "discard tmp", "delete 2"]);
}

#[test]
fn retention_days_only_lower_the_instance_days() {
    for (asked, days) in [(None, 90), (Some(3), 3), (Some(0), 1), (Some(500), 90)] {
        let store = Store::default();
        upload(&store, "a", HEX, asked, 5).unwrap();
        assert_eq!(*store.recorded.lock().unwrap(), Some((5, 1_000 + days * 86_400)));
    }
}

#[test]
fn removal_and_expiry_delete_the_file_then_the_row() {
    let store = Store::default();
    run(ArtifactStorage::remove(&store, &3)).unwrap();
    assert_eq!(run(ArtifactStorage::expire(&store, 9)).unwrap(), 2);
    assert_eq!(store.calls(), ["remove file3", "delete 3", "expired 9 256", "remove file4", "delete 4", "remove file5", "delete 5"]);
}
