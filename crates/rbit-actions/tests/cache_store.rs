//! Shared-cache decisions over a scripted, immediately ready port.
// Scripted ports answer at once; the traits still return futures.
#![allow(clippy::unused_async_trait_impl)]
use rbit_actions::{
    archives::{ArchiveFiles, ArchiveUploads, Received, Refusal, StoreError},
    caches::{
        CacheFacts, CacheLookup, CacheRecorded, CacheRecords, CacheRetention, CacheSave, CacheSaveStore, CacheTaskStore, NewCacheEntry,
        SharedCache,
    },
};
use rbit_workflow::spec::CacheScopes;
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

const HEX: &str = "abababababababababababababababababababababababababababababababab";
const VERSION: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// An entry: id, repository, scope, key, size.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry(i64, i64, String, String, u64);

#[derive(Debug, Default)]
struct Cache {
    calls: Mutex<Vec<String>>,
    entries: Vec<Entry>,
    stored: u64,
    digest: Option<[u8; 32]>,
    exists: bool,
}

impl Cache {
    fn call(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn scopes() -> CacheScopes {
        CacheScopes { repository_id: 1, read: vec!["heads/f".into(), "heads/main".into()], write: "heads/f".into() }
    }
}

impl ArchiveFiles for Cache {
    type Error = &'static str;
    type Location = String;
    async fn remove(&self, location: &String) {
        self.call(format!("remove {location}"));
    }
}

impl CacheRecords for Cache {
    type Entry = Entry;
    fn facts<'a>(&self, entry: &'a Entry) -> CacheFacts<'a> {
        CacheFacts { id: entry.0, repository: entry.1, scope: &entry.2, size: entry.4 }
    }
    fn location(&self, entry: &Entry) -> String {
        format!("file{}", entry.0)
    }
    async fn delete(&self, entry: &Entry) -> Result<(), &'static str> {
        self.call(format!("delete {}", entry.0));
        Ok(())
    }
    async fn stored_bytes(&self, _: i64) -> Result<u64, &'static str> {
        Ok(self.stored)
    }
    async fn least_used(&self, _: i64, limit: u32) -> Result<Vec<Entry>, &'static str> {
        self.call(format!("least used {limit}"));
        Ok(self.entries.clone())
    }
}

impl CacheTaskStore for Cache {
    type Target = CacheScopes;
    type Opened = &'static str;
    async fn lease(&self) -> Result<CacheScopes, &'static str> {
        self.call("lease");
        Ok(Self::scopes())
    }
    fn repository(&self, _: &CacheScopes) -> i64 {
        1
    }
    fn scopes<'a>(&self, target: &'a CacheScopes) -> &'a CacheScopes {
        target
    }
    async fn exact(&self, _: i64, scope: &str, key: &str, _: &str) -> Result<Option<Entry>, &'static str> {
        self.call(format!("exact {scope} {key}"));
        Ok(self.entries.iter().find(|entry| entry.2 == scope && entry.3 == key).cloned())
    }
    async fn newest_with_prefix(&self, _: i64, scope: &str, prefix: &str, _: &str) -> Result<Option<Entry>, &'static str> {
        self.call(format!("prefix {scope} {prefix}"));
        Ok(self.entries.iter().find(|entry| entry.2 == scope && entry.3.starts_with(prefix)).cloned())
    }
    async fn find(&self, id: i64) -> Result<Option<Entry>, &'static str> {
        Ok(self.entries.iter().find(|entry| entry.0 == id).cloned())
    }
    async fn open(&self, entry: &Entry) -> Result<&'static str, &'static str> {
        self.call(format!("open {}", entry.0));
        Ok("file")
    }
    async fn touch(&self, entry: &Entry) -> Result<(), &'static str> {
        self.call(format!("touch {}", entry.0));
        Ok(())
    }
}

impl ArchiveUploads for Cache {
    type Body = u64;
    type Temporary = &'static str;
    async fn receive(&self, body: u64, limit: u64) -> Result<Option<Received<&'static str>>, &'static str> {
        self.call(format!("receive {limit}"));
        Ok((body <= limit).then(|| Received { temporary: "tmp", size: body, sha256: self.digest.unwrap_or([0xab; 32]) }))
    }
    async fn discard(&self, _: &&'static str) {
        self.call("discard");
    }
    async fn place(&self, _: &&'static str, location: &String) -> Result<(), &'static str> {
        self.call(format!("place {location}"));
        Ok(())
    }
}

impl CacheSaveStore for Cache {
    async fn still_leased(&self, _: &CacheScopes) -> Result<(), &'static str> {
        Ok(())
    }
    async fn record(&self, new: &NewCacheEntry<'_>, replace: bool) -> Result<CacheRecorded<Entry>, &'static str> {
        self.call(format!("record {} {} {replace}", new.scope, new.key));
        let entry = Entry(9, new.repository, new.scope.into(), new.key.into(), new.size);
        Ok(if self.exists { CacheRecorded::Exists } else { CacheRecorded::Stored { entry, replaced: None } })
    }
}

impl CacheRetention for Cache {
    async fn unused_since(&self, cutoff: i64, limit: u32) -> Result<Vec<Entry>, &'static str> {
        self.call(format!("unused since {cutoff} {limit}"));
        Ok(Vec::new())
    }
}

fn entry(id: i64, scope: &str, key: &str, size: u64) -> Entry {
    Entry(id, 1, scope.into(), key.into(), size)
}

fn refused<T: std::fmt::Debug>(result: Result<T, StoreError<&'static str>>) -> Refusal {
    match result {
        Err(StoreError::Refused(refusal)) => refusal,
        other => panic!("{other:?}"),
    }
}

fn find(cache: &Cache, key: &str, restore_keys: &[String], version: &str) -> Result<Option<Entry>, StoreError<&'static str>> {
    run(SharedCache::find(cache, &CacheLookup { key, restore_keys, version }))
}

#[test]
fn lookups_are_checked_before_the_lease() {
    let cache = Cache::default();
    let keys = Refusal::BadRequest("cache keys are 1 to 512 characters without commas".into());
    assert_eq!(refused(find(&cache, "", &[], VERSION)), keys);
    assert_eq!(refused(find(&cache, "k", &["a,b".into()], VERSION)), keys);
    let many: Vec<String> = (0..11).map(|n| n.to_string()).collect();
    assert_eq!(refused(find(&cache, "k", &many, VERSION)), Refusal::BadRequest("a lookup has at most 10 restore keys".into()));
    assert_eq!(refused(find(&cache, "k", &[], "ABC")), Refusal::BadRequest("the cache version must be 64 lowercase hex digits".into()));
    assert_eq!(cache.calls(), Vec::<String>::new());
}

#[test]
fn restores_probe_exact_keys_then_prefixes_and_stop_at_the_first_hit() {
    let cache = Cache { entries: vec![entry(3, "heads/main", "cargo-old", 1)], ..Cache::default() };
    let found = find(&cache, "cargo-new", &["cargo-".into()], VERSION).unwrap();
    assert_eq!(found, Some(entry(3, "heads/main", "cargo-old", 1)));
    assert_eq!(
        cache.calls(),
        ["lease", "exact heads/f cargo-new", "exact heads/main cargo-new", "prefix heads/f cargo-", "prefix heads/main cargo-"]
    );
}

#[test]
fn a_task_opens_only_entries_of_its_repository_and_read_scopes() {
    let other_scope = entry(4, "pull/3", "k", 1);
    let other_repository = Entry(5, 2, "heads/main".into(), "k".into(), 1);
    let cache = Cache { entries: vec![entry(3, "heads/main", "k", 1), other_scope, other_repository], ..Cache::default() };
    for id in [4, 5, 6] {
        assert_eq!(refused(run(SharedCache::open(&cache, id))), Refusal::NotFound("this task has no such cache entry".into()));
    }
    assert_eq!(run(SharedCache::open(&cache, 3)).unwrap().1, "file");
    assert_eq!(cache.calls()[3..], ["lease", "open 3", "touch 3"]);
}

fn save(cache: &Cache, replace: bool, body: u64) -> Result<Option<Entry>, StoreError<&'static str>> {
    run(SharedCache::save(cache, &CacheSave { key: "k", version: VERSION, sha256: HEX, replace }, 100, body))
}

#[test]
fn saves_skip_keys_the_write_scope_has_unless_replacing() {
    let cache = Cache { entries: vec![entry(3, "heads/f", "k", 1)], ..Cache::default() };
    assert_eq!(save(&cache, false, 5).unwrap(), None);
    assert_eq!(cache.calls(), ["lease", "exact heads/f k"]);
    let replacing = Cache { entries: vec![entry(3, "heads/f", "k", 1)], ..Cache::default() };
    assert_eq!(save(&replacing, true, 5).unwrap(), Some(entry(9, "heads/f", "k", 5)));
    assert_eq!(replacing.calls(), ["lease", "receive 100", "record heads/f k true", "place file9"]);
    let raced = Cache { exists: true, ..Cache::default() };
    assert_eq!(save(&raced, false, 5).unwrap(), None);
    assert_eq!(raced.calls().last().unwrap(), "discard");
}

#[test]
fn oversized_or_mismatched_saves_are_refused() {
    let text = "the cache entry is larger than a repository's cache storage";
    assert_eq!(refused(save(&Cache::default(), false, 101)), Refusal::TooLarge(text.into()));
    let wrong = Cache { digest: Some([0; 32]), ..Cache::default() };
    assert_eq!(refused(save(&wrong, false, 5)), Refusal::BadRequest("the cache entry does not match its content digest".into()));
    assert_eq!(wrong.calls().last().unwrap(), "discard");
    let bad = run(SharedCache::save(&Cache::default(), &CacheSave { key: "k", version: VERSION, sha256: "x", replace: false }, 100, 1));
    assert_eq!(refused(bad), Refusal::BadRequest("the content digest must be 64 hex digits".into()));
}

#[test]
fn eviction_removes_least_used_entries_but_never_the_saved_one() {
    let saved = entry(9, "heads/f", "k", 50);
    let entries = vec![saved.clone(), entry(1, "heads/f", "a", 30), entry(2, "heads/f", "b", 30), entry(3, "heads/f", "c", 30)];
    let cache = Cache { entries, stored: 140, ..Cache::default() };
    run(SharedCache::evict(&cache, &saved, 100)).unwrap();
    assert_eq!(cache.calls(), ["least used 256", "remove file1", "delete 1", "remove file2", "delete 2"]);
    let within = Cache { stored: 100, ..Cache::default() };
    run(SharedCache::evict(&within, &saved, 100)).unwrap();
    assert_eq!(within.calls(), Vec::<String>::new());
}

#[test]
fn expiry_counts_retention_days_back_from_now() {
    let cache = Cache::default();
    assert_eq!(run(SharedCache::expire(&cache, 1_000_000, 7)).unwrap(), 0);
    assert_eq!(cache.calls(), ["unused since 395200 256"]);
}
