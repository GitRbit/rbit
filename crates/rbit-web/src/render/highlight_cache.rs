//! A byte-bounded, least-recently-used cache of highlighted files.
//!
//! Highlighting is the most expensive part of the file viewer and blame (seconds for a
//! 20,000-line file in a debug build), and its input never changes for a given blob: the
//! key is the content's id (a Git blob id, or a Git LFS object id), the file name (which
//! picks the syntax) and [`HIGHLIGHTER_VERSION`], so a server that renders differently
//! never serves old markup. Nothing needs invalidating; old entries simply age out.
//!
//! Memory is bounded by [`DEFAULT_MAX_BYTES`], counted over the stored strings; an entry
//! larger than a quarter of the budget is not kept, so one huge file cannot flush
//! everything else.
//! Concurrent misses for one key share a blocking calculation. At most four distinct
//! jobs are admitted; cancelled requests leave their job registered until it actually
//! finishes, and waiters never occupy blocking threads.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
    sync::{Arc, Mutex, PoisonError},
};

use tokio::sync::watch;

use crate::error::{WebError, WebResult};

/// Part of every key: bump it whenever `Highlighter::highlight_lines` changes its output.
pub const HIGHLIGHTER_VERSION: u32 = 3;

/// 32 MiB. Compact markup is about four times its source text, so the largest file the
/// viewer highlights (512 KiB of source, ~2 MiB of markup) takes 1/16 of it, and a
/// typical source file (tens of KiB) a few hundred KiB: dozens of recently viewed files
/// stay warm at a cost that is small next to the server's other caches.
pub const DEFAULT_MAX_BYTES: usize = 32 * 1024 * 1024;

/// Both map keys, entry fields, use tick, Arc header and allowance for map nodes.
const ENTRY_OVERHEAD: usize = 2 * size_of::<Key>() + size_of::<Entry>() + size_of::<u64>() + size_of::<[usize; 8]>();
const MAX_JOBS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Key {
    version: u32,
    content_id: String,
    file_name: String,
}

#[derive(Debug)]
struct Entry {
    lines: Arc<[String]>,
    bytes: usize,
    /// When it was last used; its position in [`State::by_use`].
    used: u64,
}

#[derive(Debug, Default)]
struct State {
    entries: HashMap<Key, Entry>,
    running: HashMap<Key, watch::Receiver<Option<Arc<[String]>>>>,
    /// Use tick → key, least recently used first.
    by_use: BTreeMap<u64, Key>,
    clock: u64,
    bytes: usize,
    hits: u64,
    misses: u64,
}

impl State {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

/// Hit and miss counts, for tests and diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct HighlightCache {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    max_bytes: usize,
    max_jobs: usize,
    state: Mutex<State>,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_BYTES)
    }
}

impl HighlightCache {
    pub fn new(max_bytes: usize) -> Self {
        Self::bounded(max_bytes, MAX_JOBS)
    }

    fn bounded(max_bytes: usize, max_jobs: usize) -> Self {
        Self { inner: Arc::new(Inner { max_bytes, max_jobs, state: Mutex::default() }) }
    }

    fn key(content_id: &str, file_name: &str) -> Key {
        Key { version: HIGHLIGHTER_VERSION, content_id: content_id.to_owned(), file_name: file_name.to_owned() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // The state stays consistent between statements, so a panic elsewhere cannot
        // leave it half-updated in a way that matters; keep serving.
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The highlighted lines stored for this content and name, marking them recently used.
    pub fn get(&self, content_id: &str, file_name: &str) -> Option<Arc<[String]>> {
        let key = Self::key(content_id, file_name);
        let mut state = self.lock();
        Self::lookup(&mut state, &key)
    }

    fn lookup(state: &mut State, key: &Key) -> Option<Arc<[String]>> {
        let now = state.tick();
        let Some(entry) = state.entries.get_mut(key) else {
            state.misses += 1;
            return None;
        };
        let (previous, lines) = (entry.used, Arc::clone(&entry.lines));
        entry.used = now;
        state.by_use.remove(&previous);
        state.by_use.insert(now, key.clone());
        state.hits += 1;
        Some(lines)
    }

    /// Stores `lines`, evicting the least recently used entries beyond the budget.
    pub fn insert(&self, content_id: &str, file_name: &str, lines: Arc<[String]>) {
        let key = Self::key(content_id, file_name);
        let bytes = retained_bytes(&key, &lines);
        self.store(&mut self.lock(), key, lines, bytes);
    }

    fn store(&self, state: &mut State, key: Key, lines: Arc<[String]>, bytes: usize) {
        if bytes > self.inner.max_bytes / 4 {
            return;
        }
        let now = state.tick();
        if let Some(old) = state.entries.insert(key.clone(), Entry { lines, bytes, used: now }) {
            state.by_use.remove(&old.used);
            state.bytes -= old.bytes;
        }
        state.by_use.insert(now, key);
        state.bytes += bytes;
        while state.bytes > self.inner.max_bytes {
            let Some((_, oldest)) = state.by_use.pop_first() else { break };
            if let Some(evicted) = state.entries.remove(&oldest) {
                state.bytes -= evicted.bytes;
            }
        }
    }

    /// Returns shared markup, or runs one CPU job for this key on the blocking pool.
    /// Excess distinct jobs are rejected instead of accumulating an unbounded queue.
    pub async fn get_or_compute<F>(&self, content_id: &str, file_name: &str, compute: F) -> WebResult<Arc<[String]>>
    where
        F: FnOnce() -> Vec<String> + Send + 'static,
    {
        let key = Self::key(content_id, file_name);
        let (mut receiver, flight) = {
            let mut state = self.lock();
            if let Some(lines) = Self::lookup(&mut state, &key) {
                return Ok(lines);
            }
            if let Some(receiver) = state.running.get(&key) {
                (receiver.clone(), None)
            } else {
                if state.running.len() >= self.inner.max_jobs {
                    return Err(WebError::TooManyRequests { retry_after_seconds: 1 });
                }
                let (sender, receiver) = watch::channel(None);
                state.running.insert(key.clone(), receiver.clone());
                // Construct the guard before spawning so even a queued task dropped
                // during runtime shutdown removes its registration and closes waiters.
                let flight = Flight { cache: self.clone(), key: Some(key), sender };
                (receiver, Some(flight))
            }
        };
        // Spawning or dropping a rejected task can run destructors; neither happens
        // under the cache lock. Joining a flight releases its unused source at once.
        if let Some(mut flight) = flight {
            drop(tokio::task::spawn_blocking(move || {
                let lines: Arc<[String]> = compute().into();
                flight.complete(lines);
            }));
        } else {
            drop(compute);
        }
        loop {
            if let Some(lines) = receiver.borrow().clone() {
                return Ok(lines);
            }
            receiver.changed().await.map_err(WebError::internal)?;
        }
    }

    pub fn stats(&self) -> CacheStats {
        let state = self.lock();
        CacheStats { hits: state.hits, misses: state.misses, entries: state.entries.len(), bytes: state.bytes }
    }
}

/// Lives in the actual CPU job, independently of any response future.
struct Flight {
    cache: HighlightCache,
    key: Option<Key>,
    sender: watch::Sender<Option<Arc<[String]>>>,
}

impl Flight {
    fn complete(&mut self, lines: Arc<[String]>) {
        let Some(key) = self.key.as_ref() else { return };
        let bytes = retained_bytes(key, &lines);
        let Some(key) = self.key.take() else { return };
        let mut state = self.cache.lock();
        state.running.remove(&key);
        self.cache.store(&mut state, key, Arc::clone(&lines), bytes);
        self.sender.send_replace(Some(lines));
    }
}

fn retained_bytes(key: &Key, lines: &[String]) -> usize {
    // The LRU map owns a second key. Count capacities and empty-line headers, rather
    // than just visible markup. Walk the lines outside the shared cache lock.
    lines.iter().fold(
        ENTRY_OVERHEAD.saturating_add(key.content_id.capacity().saturating_add(key.file_name.capacity()).saturating_mul(2)),
        |bytes, line| bytes.saturating_add(size_of::<String>()).saturating_add(line.capacity()),
    )
}

impl Drop for Flight {
    fn drop(&mut self) {
        if let Some(key) = &self.key {
            self.cache.lock().running.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::Duration,
    };

    use futures_util::poll;
    use tokio::sync::oneshot;

    use super::*;

    fn lines(count: usize, width: usize) -> Arc<[String]> {
        (0..count).map(|_| "x".repeat(width)).collect()
    }

    #[test]
    fn hits_return_the_stored_lines() {
        let cache = HighlightCache::default();
        assert!(cache.get("blob1", "a.rs").is_none());
        cache.insert("blob1", "a.rs", lines(3, 10));
        let found = cache.get("blob1", "a.rs").unwrap();
        assert_eq!(found.len(), 3);
        assert!(cache.get("blob1", "a.txt").is_none(), "the file name picks the syntax");
        assert!(cache.get("blob2", "a.rs").is_none());
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses, stats.entries), (1, 3, 1));
    }

    #[test]
    fn least_recently_used_entries_are_evicted_first() {
        // Exactly enough room for five entries of this shape.
        let budget = 5 * retained_bytes(&HighlightCache::key("a", "f.rs"), &lines(1, 600));
        let cache = HighlightCache::new(budget);
        for id in ["a", "b", "c", "d", "e"] {
            cache.insert(id, "f.rs", lines(1, 600));
        }
        assert_eq!(cache.stats().entries, 5);
        // Touch `a`, so `b` is now the least recently used.
        assert!(cache.get("a", "f.rs").is_some());
        cache.insert("f", "f.rs", lines(1, 600));
        assert!(cache.get("b", "f.rs").is_none(), "evicted");
        for id in ["a", "c", "d", "e", "f"] {
            assert!(cache.get(id, "f.rs").is_some(), "{id} kept");
        }
        assert!(cache.stats().bytes <= budget);
    }

    #[test]
    fn replacing_an_entry_keeps_the_byte_count_exact() {
        let cache = HighlightCache::new(1 << 20);
        cache.insert("a", "f.rs", lines(10, 10));
        let once = cache.stats().bytes;
        cache.insert("a", "f.rs", lines(10, 10));
        assert_eq!(cache.stats().bytes, once);
        assert_eq!(cache.stats().entries, 1);
    }

    #[test]
    fn entries_over_a_quarter_of_the_budget_are_not_kept() {
        let cache = HighlightCache::new(4096);
        cache.insert("big", "f.rs", lines(4, 512));
        assert!(cache.get("big", "f.rs").is_none());
        assert_eq!(cache.stats().bytes, 0);
    }

    #[test]
    fn accounting_includes_spare_capacity_empty_lines_and_both_keys() {
        let cache = HighlightCache::default();
        let mut large = String::with_capacity(4096);
        large.push('x');
        let capacity = large.capacity();
        let lines: Arc<[String]> = [large, String::new()].into();
        cache.insert("id", "a.rs", Arc::clone(&lines));
        assert_eq!(cache.stats().bytes, ENTRY_OVERHEAD + 2 * (2 + 4) + 2 * size_of::<String>() + capacity);
        assert!(Arc::ptr_eq(&lines, &cache.get("id", "a.rs").unwrap()));
    }

    #[tokio::test]
    async fn concurrent_cold_requests_share_one_computation_and_allocation() {
        struct UnusedSource(Arc<AtomicUsize>);
        impl Drop for UnusedSource {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let cache = HighlightCache::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let (started, start) = oneshot::channel();
        let (release, blocked) = mpsc::channel();
        let first = {
            let cache = cache.clone();
            let calls = Arc::clone(&calls);
            tokio::spawn(async move {
                cache
                    .get_or_compute("id", "a.rs", move || {
                        calls.fetch_add(1, Ordering::Relaxed);
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                        vec!["shared".into()]
                    })
                    .await
                    .unwrap()
            })
        };
        start.await.unwrap();
        let unused_drops = Arc::new(AtomicUsize::new(0));
        let unused = UnusedSource(Arc::clone(&unused_drops));
        let second = cache.get_or_compute("id", "a.rs", move || {
            drop(unused);
            panic!("duplicate CPU job")
        });
        tokio::pin!(second);
        assert!(poll!(&mut second).is_pending());
        assert_eq!(unused_drops.load(Ordering::Relaxed), 1, "a joined request does not retain another source buffer");
        release.send(()).unwrap();
        let first = first.await.unwrap();
        let second = second.await.unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(Arc::ptr_eq(&first, &second));
        let warm = cache.get_or_compute("id", "a.rs", || panic!("warm CPU job")).await.unwrap();
        assert!(Arc::ptr_eq(&first, &warm));
        assert_eq!(cache.stats().entries, 1);
    }

    #[tokio::test]
    async fn unrelated_keys_finish_while_another_job_is_blocked() {
        let cache = HighlightCache::bounded(DEFAULT_MAX_BYTES, 2);
        let (started, start) = oneshot::channel();
        let (release, blocked) = mpsc::channel();
        let first = {
            let cache = cache.clone();
            tokio::spawn(async move {
                cache
                    .get_or_compute("id", "a.rs", move || {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                        vec!["rust".into()]
                    })
                    .await
                    .unwrap()
            })
        };
        start.await.unwrap();
        let other = tokio::time::timeout(Duration::from_secs(2), cache.get_or_compute("id", "a.txt", || vec!["plain".into()]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(other.as_ref(), ["plain"]);
        release.send(()).unwrap();
        assert_eq!(first.await.unwrap().as_ref(), ["rust"]);
    }

    #[tokio::test]
    async fn cancelled_waiters_do_not_release_capacity_or_duplicate_work() {
        let cache = HighlightCache::bounded(DEFAULT_MAX_BYTES, 1);
        let (started, start) = oneshot::channel();
        let (release, blocked) = mpsc::channel();
        let first = {
            let cache = cache.clone();
            tokio::spawn(async move {
                cache
                    .get_or_compute("id", "a.rs", move || {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                        vec!["shared".into()]
                    })
                    .await
            })
        };
        start.await.unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        let denied = cache.get_or_compute("other", "a.rs", || panic!("capacity exceeded")).await;
        assert!(matches!(denied, Err(WebError::TooManyRequests { .. })));
        let joined = cache.get_or_compute("id", "a.rs", || panic!("cancelled job duplicated"));
        tokio::pin!(joined);
        assert!(poll!(&mut joined).is_pending());
        release.send(()).unwrap();
        assert_eq!(joined.await.unwrap().as_ref(), ["shared"]);
        assert_eq!(cache.get_or_compute("other", "a.rs", || vec!["next".into()]).await.unwrap().as_ref(), ["next"]);
    }

    #[tokio::test]
    async fn panicked_jobs_wake_waiters_release_capacity_and_allow_retry() {
        let cache = HighlightCache::bounded(DEFAULT_MAX_BYTES, 1);
        let failed = tokio::time::timeout(Duration::from_secs(2), cache.get_or_compute("id", "a.rs", || panic!("simulated parser panic")))
            .await
            .unwrap();
        assert!(matches!(failed, Err(WebError::Internal(_))));
        assert!(cache.lock().running.is_empty());
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.get_or_compute("id", "a.rs", || vec!["retry".into()]).await.unwrap().as_ref(), ["retry"]);
    }

    #[tokio::test]
    async fn oversized_results_are_returned_without_retaining_them() {
        let cache = HighlightCache::new(1024);
        let found = cache.get_or_compute("id", "a.rs", || vec!["x".repeat(1024)]).await.unwrap();
        assert_eq!(found[0].len(), 1024);
        assert_eq!(cache.stats().bytes, 0);
        assert!(cache.lock().running.is_empty());
    }

    #[test]
    fn shared_between_threads() {
        let cache = Arc::new(HighlightCache::default());
        let handles: Vec<_> = (0..4)
            .map(|thread| {
                let cache = Arc::clone(&cache);
                std::thread::spawn(move || {
                    for round in 0..50 {
                        let id = format!("{thread}-{}", round % 5);
                        if cache.get(&id, "f.rs").is_none() {
                            cache.insert(&id, "f.rs", lines(2, 8));
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let stats = cache.stats();
        assert_eq!(stats.entries, 20);
        assert_eq!(stats.hits + stats.misses, 200);
    }
}
