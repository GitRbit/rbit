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

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, PoisonError},
};

/// Part of every key: bump it whenever `Highlighter::highlight_lines` changes its output.
pub const HIGHLIGHTER_VERSION: u32 = 3;

/// 32 MiB. Compact markup is about four times its source text, so the largest file the
/// viewer highlights (512 KiB of source, ~2 MiB of markup) takes 1/16 of it, and a
/// typical source file (tens of KiB) a few hundred KiB: dozens of recently viewed files
/// stay warm at a cost that is small next to the server's other caches.
pub const DEFAULT_MAX_BYTES: usize = 32 * 1024 * 1024;

/// Bookkeeping counted per entry on top of its strings (map nodes, the key, the `Arc`).
const ENTRY_OVERHEAD: usize = 128;

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

#[derive(Debug)]
pub struct HighlightCache {
    max_bytes: usize,
    state: Mutex<State>,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_BYTES)
    }
}

impl HighlightCache {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes, state: Mutex::new(State::default()) }
    }

    fn key(content_id: &str, file_name: &str) -> Key {
        Key { version: HIGHLIGHTER_VERSION, content_id: content_id.to_owned(), file_name: file_name.to_owned() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // The state stays consistent between statements, so a panic elsewhere cannot
        // leave it half-updated in a way that matters; keep serving.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The highlighted lines stored for this content and name, marking them recently used.
    pub fn get(&self, content_id: &str, file_name: &str) -> Option<Arc<[String]>> {
        let key = Self::key(content_id, file_name);
        let mut state = self.lock();
        let now = state.tick();
        let Some(entry) = state.entries.get_mut(&key) else {
            state.misses += 1;
            return None;
        };
        let (previous, lines) = (entry.used, Arc::clone(&entry.lines));
        entry.used = now;
        state.by_use.remove(&previous);
        state.by_use.insert(now, key);
        state.hits += 1;
        Some(lines)
    }

    /// Stores `lines`, evicting the least recently used entries beyond the budget.
    pub fn insert(&self, content_id: &str, file_name: &str, lines: Arc<[String]>) {
        let key = Self::key(content_id, file_name);
        let bytes = ENTRY_OVERHEAD
            + key.content_id.len()
            + key.file_name.len()
            + lines.iter().map(|line| line.capacity() + std::mem::size_of::<String>()).sum::<usize>();
        if bytes > self.max_bytes / 4 {
            return;
        }
        let mut state = self.lock();
        let now = state.tick();
        if let Some(old) = state.entries.insert(key.clone(), Entry { lines, bytes, used: now }) {
            state.by_use.remove(&old.used);
            state.bytes -= old.bytes;
        }
        state.by_use.insert(now, key);
        state.bytes += bytes;
        while state.bytes > self.max_bytes {
            let Some((_, oldest)) = state.by_use.pop_first() else { break };
            if let Some(evicted) = state.entries.remove(&oldest) {
                state.bytes -= evicted.bytes;
            }
        }
    }

    pub fn stats(&self) -> CacheStats {
        let state = self.lock();
        CacheStats { hits: state.hits, misses: state.misses, entries: state.entries.len(), bytes: state.bytes }
    }
}

#[cfg(test)]
mod tests {
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
        // Each entry counts ~760 bytes; room for five.
        let cache = HighlightCache::new(4000);
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
        assert!(cache.stats().bytes <= 4000);
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
