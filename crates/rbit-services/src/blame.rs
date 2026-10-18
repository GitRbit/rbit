//! Bounded reuse of immutable blame results and shared work for concurrent requests.
//!
//! Callers first obtain an authorized repository context. Neither permissions nor
//! repository readers are cached. Jobs own their work independently of response
//! deadlines, and their permits live inside the actual blocking Git calculation.

use std::{
    collections::HashMap,
    future::Future,
    io::{self, Read},
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rbit_core::{AccountId, RepoId};
use rbit_git::{
    CommitSummary, GitError,
    blame::{Blame, BlameCacheInputs, BlameHunk, BlameLimits, FileBlame},
};
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};

use crate::{RepoContext, ServiceError};

const MAX_ENTRIES: usize = 128;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_JOBS: usize = 4;
const MAX_INPUT_BYTES: u64 = 1024 * 1024;

/// A result and the immutable revision used to produce it, including fallback views.
#[derive(Debug)]
pub struct ResolvedBlame {
    pub commit_id: String,
    pub result: Arc<Blame>,
}

#[derive(Debug, Clone)]
pub struct BlameService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    state: Mutex<State>,
    permits: Arc<Semaphore>,
    max_entries: usize,
    max_bytes: usize,
    max_jobs: usize,
    next_request: AtomicU64,
}

#[derive(Debug, Default)]
struct State {
    ready: HashMap<Key, Cached>,
    running: HashMap<Key, watch::Receiver<Option<SharedResult>>>,
    bytes: usize,
    clock: u64,
}

#[derive(Debug)]
struct Cached {
    value: Arc<Blame>,
    bytes: usize,
    used: u64,
}

/// Keeping the directory handle open prevents OS file-id reuse. These handles use
/// delete sharing on Windows and retain no Git object files or repository leases.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    repository: Arc<same_file::Handle>,
    repo: RepoId,
    owner: AccountId,
    location: PathBuf,
    inputs: Option<Inputs>,
    // Unsupported input gets a unique job and can never hit or populate the cache.
    request: u64,
    commit: String,
    path: String,
    max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Inputs {
    effective: BlameCacheInputs,
    local: LocalInputs,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocalInputs([Option<[u8; 32]>; 3]);

struct Computed {
    value: Blame,
    cacheable: bool,
}

type SharedResult = Result<Arc<Blame>, Arc<GitError>>;

impl BlameService {
    pub(crate) fn new() -> Self {
        Self::bounded(MAX_ENTRIES, MAX_BYTES, MAX_JOBS)
    }

    fn bounded(max_entries: usize, max_bytes: usize, max_jobs: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::default(),
                permits: Arc::new(Semaphore::new(max_jobs)),
                max_entries,
                max_bytes,
                max_jobs,
                next_request: AtomicU64::new(1),
            }),
        }
    }

    /// Reads blame only for an already authorized repository. Every request resolves
    /// its ref again, so moving branches cannot hit results from their former tip.
    pub async fn blame(&self, repo: &RepoContext, revision: &str, path: &str) -> Result<ResolvedBlame, ServiceError> {
        if !repo.access.can_read() {
            return Err(ServiceError::NotFound);
        }
        let limits = BlameLimits::default();
        let reader = repo.reader.pinned();
        let commit_id = reader.commit_id(revision.to_owned()).await?;
        let effective = reader.blame_cache_inputs().await?;
        let location = repo.disk_path.clone();
        let (repository, local) = tokio::task::spawn_blocking(move || repository_identity(&location))
            .await
            .map_err(ServiceError::internal)?
            .map_err(ServiceError::internal)?;
        let inputs = effective.zip(local).map(|(effective, local)| Inputs { effective, local });
        let request = if inputs.is_some() { 0 } else { self.inner.next_request.fetch_add(1, Ordering::Relaxed) };
        let key = Key {
            repository: Arc::new(repository),
            repo: repo.repo.repo.id,
            owner: repo.repo.repo.owner_id,
            location: repo.disk_path.clone(),
            inputs: inputs.clone(),
            request,
            commit: commit_id.clone(),
            path: path.to_owned(),
            max_file_bytes: limits.max_file_bytes,
        };
        let commit = commit_id.clone();
        let path = path.to_owned();
        let location = repo.disk_path.clone();
        let result = self
            .get_or_compute(key, limits.time_budget, move |permit| async move {
                let value = reader.blame_complete(commit, path, limits.max_file_bytes, Some(permit)).await?;
                // Inputs can change while Git walks history. Return this calculation
                // to its waiters, but do not retain it under the earlier fingerprint.
                let cacheable = if let Some(expected) = inputs {
                    let effective = reader.blame_cache_inputs().await.ok().flatten();
                    let local = tokio::task::spawn_blocking(move || local_inputs(&location)).await.ok().flatten();
                    effective.zip(local).is_some_and(|(effective, local)| expected == Inputs { effective, local })
                } else {
                    false
                };
                Ok(Computed { value, cacheable })
            })
            .await?;
        Ok(ResolvedBlame { commit_id, result })
    }

    async fn get_or_compute<F, W>(&self, key: Key, deadline: Duration, work: F) -> Result<Arc<Blame>, ServiceError>
    where
        F: FnOnce(OwnedSemaphorePermit) -> W + Send + 'static,
        W: Future<Output = Result<Computed, GitError>> + Send + 'static,
    {
        let mut receiver = {
            let mut state = self.inner.state();
            state.clock = state.clock.saturating_add(1);
            let used = state.clock;
            if let Some(cached) = state.ready.get_mut(&key) {
                cached.used = used;
                return Ok(Arc::clone(&cached.value));
            }
            if let Some(running) = state.running.get(&key) {
                running.clone()
            } else {
                // No unbounded queue of unique calculations behind the semaphore.
                if state.running.len() >= self.inner.max_jobs {
                    return Err(ServiceError::RateLimited { retry_after_seconds: 1 });
                }
                let permit = Arc::clone(&self.inner.permits)
                    .try_acquire_owned()
                    .map_err(|_| ServiceError::RateLimited { retry_after_seconds: 1 })?;
                let (sender, receiver) = watch::channel(None);
                state.running.insert(key.clone(), receiver.clone());
                let mut flight = Flight { inner: Arc::clone(&self.inner), key: Some(key) };
                tokio::spawn(async move {
                    let result = work(permit).await;
                    let cacheable = result.as_ref().is_ok_and(|computed| computed.cacheable);
                    let result = result.map(|computed| Arc::new(computed.value)).map_err(Arc::new);
                    flight.complete(&result, cacheable);
                    sender.send_replace(Some(result));
                });
                receiver
            }
        };
        let waiting = async {
            loop {
                if let Some(result) = receiver.borrow().clone() {
                    return result.map_err(|error| {
                        if matches!(error.as_ref(), GitError::NotFound(_)) { ServiceError::NotFound } else { ServiceError::internal(error) }
                    });
                }
                receiver.changed().await.map_err(ServiceError::internal)?;
            }
        };
        // A response deadline never becomes a persistent TooLarge entry. The shared
        // calculation remains bounded by its permit and may supply a later request.
        match tokio::time::timeout(deadline, waiting).await {
            Ok(result) => result,
            Err(_) => Ok(Arc::new(Blame::TooLarge)),
        }
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Removes in-flight registration even if an unexpected task panic closes its sender.
struct Flight {
    inner: Arc<Inner>,
    key: Option<Key>,
}

impl Flight {
    fn complete(&mut self, result: &SharedResult, cacheable: bool) {
        let Some(key) = self.key.take() else { return };
        let mut state = self.inner.state();
        state.running.remove(&key);
        if !cacheable || key.request != 0 {
            return;
        }
        let Ok(value) = result else { return };
        let bytes = retained_bytes(&key, value);
        if bytes > self.inner.max_bytes || self.inner.max_entries == 0 {
            return;
        }
        while state.ready.len() >= self.inner.max_entries || state.bytes > self.inner.max_bytes - bytes {
            let Some(oldest) = state.ready.iter().min_by_key(|(_, cached)| cached.used).map(|(key, _)| key.clone()) else { break };
            if let Some(removed) = state.ready.remove(&oldest) {
                state.bytes -= removed.bytes;
            }
        }
        state.clock = state.clock.saturating_add(1);
        let used = state.clock;
        state.bytes += bytes;
        state.ready.insert(key, Cached { value: Arc::clone(value), bytes, used });
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        if let Some(key) = &self.key {
            self.inner.state().running.remove(key);
        }
    }
}

fn repository_identity(path: &Path) -> io::Result<(same_file::Handle, Option<LocalInputs>)> {
    let identity = same_file::Handle::from_path(path)?;
    Ok((identity, local_inputs(path)))
}

fn local_inputs(path: &Path) -> Option<LocalInputs> {
    Some(LocalInputs([
        file_digest(&path.join("shallow")).ok()?,
        file_digest(&path.join("info/attributes")).ok()?,
        file_digest(&path.join("index")).ok()?,
    ]))
}

fn file_digest(path: &Path) -> io::Result<Option<[u8; 32]>> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let before = file.metadata()?;
    if !before.is_file() || before.len() > MAX_INPUT_BYTES {
        return Err(io::Error::other("blame input exceeds fingerprint limit"));
    }
    let modified = before.modified()?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    let mut bytes = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += u64::try_from(count).unwrap_or(u64::MAX);
        if bytes > MAX_INPUT_BYTES {
            return Err(io::Error::other("blame input exceeds fingerprint limit"));
        }
        digest.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if bytes != before.len() || after.len() != bytes || after.modified()? != modified {
        return Err(io::Error::other("blame input changed while fingerprinting"));
    }
    Ok(Some(digest.finalize().into()))
}

fn retained_bytes(key: &Key, value: &Blame) -> usize {
    let mut bytes = size_of::<Key>()
        + size_of::<Cached>()
        + size_of::<Blame>()
        + size_of::<same_file::Handle>()
        + 4 * size_of::<usize>()
        + key.location.capacity()
        + key.commit.capacity()
        + key.path.capacity();
    if let Blame::Blamed(file) = value {
        bytes = bytes.saturating_add(
            size_of::<FileBlame>()
                + file.text.capacity()
                + file.hunks.capacity().saturating_mul(size_of::<BlameHunk>())
                + commit_bytes(&file.commit),
        );
        for hunk in &file.hunks {
            bytes = bytes.saturating_add(commit_bytes(&hunk.commit) + hunk.source_path.as_ref().map_or(0, String::capacity));
        }
    }
    bytes
}

fn commit_bytes(commit: &CommitSummary) -> usize {
    commit.id.capacity()
        + commit.short_id.capacity()
        + commit.summary.capacity()
        + commit.body.as_ref().map_or(0, String::capacity)
        + commit.author_name.capacity()
        + commit.author_email.capacity()
        + commit.parent_ids.capacity() * size_of::<String>()
        + commit.parent_ids.iter().map(String::capacity).sum::<usize>()
}

#[cfg(test)]
mod tests;
