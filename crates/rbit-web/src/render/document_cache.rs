//! A byte-bounded, least-recently-used cache of rendered Markdown documents.
//!
//! READMEs and Markdown files are rendered on every view of a tree or file, and a long
//! document takes far longer to parse, render and sanitise than to copy. The output
//! depends only on the source and on the [`DocumentBase`] its relative links resolve
//! against, so the key is a SHA-256 digest of the source (a content id like a blob id,
//! which the renderer does not receive) and the base's every field, plus
//! [`RENDERER_VERSION`]. Nothing needs invalidating: an edited document has another
//! digest, and old entries age out.
//!
//! Only documents with a base go through it (READMEs, Markdown files, release notes,
//! milestone descriptions); comments are rendered by
//! [`super::render_markdown_rewriting`] and never cached. Sources shorter than
//! [`MIN_SOURCE_BYTES`] are rendered directly: they are cheap, and caching them would
//! only churn the budget.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
    sync::{Arc, Mutex, PoisonError},
};

use sha2::{Digest, Sha256};

use crate::urls::DocumentBase;

/// Part of every key: bump it whenever rendering changes its output.
pub const RENDERER_VERSION: u32 = 1;

/// 8 MiB: rendered documents are about their source's size, so hundreds of READMEs fit.
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Sources below this are rendered on every call.
pub const MIN_SOURCE_BYTES: usize = 512;

/// Both map keys, entry fields, use tick, Arc header and allowance for map nodes.
const ENTRY_OVERHEAD: usize = 2 * size_of::<Key>() + size_of::<Entry>() + size_of::<u64>() + size_of::<[usize; 8]>();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    version: u32,
    digest: [u8; 32],
    /// Every field of the base, as `Debug` spells them: repository, revision, directory.
    base: String,
}

#[derive(Debug)]
struct Entry {
    html: Arc<str>,
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

/// Hit and miss counts, for tests and diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub bytes: usize,
}

#[derive(Debug)]
pub struct DocumentCache {
    max_bytes: usize,
    state: Mutex<State>,
}

impl Default for DocumentCache {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_BYTES)
    }
}

impl DocumentCache {
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes, state: Mutex::default() }
    }

    /// The process-wide cache [`super::render_markdown`] uses.
    pub fn shared() -> &'static Self {
        static SHARED: std::sync::OnceLock<DocumentCache> = std::sync::OnceLock::new();
        SHARED.get_or_init(Self::default)
    }

    fn key(source: &str, base: &DocumentBase) -> Key {
        Key { version: RENDERER_VERSION, digest: Sha256::digest(source.as_bytes()).into(), base: format!("{base:?}") }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // The state stays consistent between statements; keep serving after a panic.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The rendering of `source` at `base`: stored, or made by `render` (outside the lock)
    /// and stored.
    pub fn get_or_render(&self, source: &str, base: &DocumentBase, render: impl FnOnce() -> String) -> String {
        if source.len() < MIN_SOURCE_BYTES {
            return render();
        }
        let key = Self::key(source, base);
        if let Some(html) = self.lookup(&key) {
            return html.to_string();
        }
        let html = render();
        self.store(key, Arc::from(html.as_str()));
        html
    }

    fn lookup(&self, key: &Key) -> Option<Arc<str>> {
        let mut state = self.lock();
        state.clock += 1;
        let now = state.clock;
        let Some(entry) = state.entries.get_mut(key) else {
            state.misses += 1;
            return None;
        };
        let (previous, html) = (entry.used, Arc::clone(&entry.html));
        entry.used = now;
        state.by_use.remove(&previous);
        state.by_use.insert(now, key.clone());
        state.hits += 1;
        Some(html)
    }

    fn store(&self, key: Key, html: Arc<str>) {
        let bytes = ENTRY_OVERHEAD.saturating_add(key.base.len().saturating_mul(2)).saturating_add(html.len());
        if bytes > self.max_bytes / 4 {
            return;
        }
        let mut state = self.lock();
        state.clock += 1;
        let now = state.clock;
        if let Some(old) = state.entries.insert(key.clone(), Entry { html, bytes, used: now }) {
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

    #[must_use]
    pub fn stats(&self) -> DocumentCacheStats {
        let state = self.lock();
        DocumentCacheStats { hits: state.hits, misses: state.misses, entries: state.entries.len(), bytes: state.bytes }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::urls::RepoUrls;

    fn base(revision: &str, directory: &str) -> DocumentBase {
        DocumentBase::new(RepoUrls::new("alice", "proj"), revision, directory)
    }

    fn document(word: &str) -> String {
        format!("# {word}\n\n{}", "Some text with a [link](guide.md). ".repeat(40))
    }

    #[test]
    fn a_document_renders_once_per_source_and_base() {
        let cache = DocumentCache::default();
        let renders = Cell::new(0);
        let render = |source: &str| {
            renders.set(renders.get() + 1);
            format!("<p>{}</p>", source.len())
        };
        let readme = document("Readme");
        let first = cache.get_or_render(&readme, &base("main", ""), || render(&readme));
        let again = cache.get_or_render(&readme, &base("main", ""), || render(&readme));
        assert_eq!((first.as_str(), renders.get()), (again.as_str(), 1), "the second view reuses the first rendering");
        // Anything that changes links, or the text itself, renders again.
        for (source, at) in [(&readme, base("v1", "")), (&readme, base("main", "docs")), (&document("Other"), base("main", ""))] {
            cache.get_or_render(source, &at, || render(source));
        }
        assert_eq!(renders.get(), 4);
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses, stats.entries), (1, 4, 4));
    }

    #[test]
    fn short_documents_are_not_kept() {
        let cache = DocumentCache::default();
        let renders = Cell::new(0);
        for _ in 0..2 {
            cache.get_or_render("Short notes.", &base("main", ""), || {
                renders.set(renders.get() + 1);
                String::new()
            });
        }
        assert_eq!((renders.get(), cache.stats().entries), (2, 0));
    }

    #[test]
    fn memory_stays_within_the_budget_least_recently_used_first() {
        let one = ENTRY_OVERHEAD + 2 * format!("{:?}", base("main", "")).len() + 1000;
        let cache = DocumentCache::new(4 * one);
        let html = "x".repeat(1000);
        let sources: Vec<String> = (0..5).map(|n| document(&n.to_string())).collect();
        for source in &sources[..4] {
            cache.get_or_render(source, &base("main", ""), || html.clone());
        }
        // Touch the first, so the second is the least recently used.
        cache.get_or_render(&sources[0], &base("main", ""), || panic!("cached"));
        cache.get_or_render(&sources[4], &base("main", ""), || html.clone());
        assert!(cache.stats().bytes <= 4 * one);
        cache.get_or_render(&sources[0], &base("main", ""), || panic!("kept"));
        let rendered = Cell::new(false);
        cache.get_or_render(&sources[1], &base("main", ""), || {
            rendered.set(true);
            html.clone()
        });
        assert!(rendered.get(), "the least recently used entry was evicted");
        // An entry over a quarter of the budget is returned but never kept.
        let huge = DocumentCache::new(4096);
        assert_eq!(huge.get_or_render(&sources[0], &base("main", ""), || "y".repeat(2048)).len(), 2048);
        assert_eq!(huge.stats().bytes, 0);
    }
}
