//! Short-lived, bounded reuse of directory history walks, including incomplete ones.
//! Admission is acquired before spawning. The worker owns the permit and slot even
//! when its HTTP request is cancelled, so blocking Git work cannot escape the limit.

use crate::ServiceError;
use rbit_git::LastCommits;
use std::{
    collections::VecDeque,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WalkKey {
    pub repository: rbit_core::RepoId,
    pub commit: String,
    pub path: String,
    pub names: Vec<String>,
}

#[derive(Debug)]
struct Cached {
    value: Arc<LastCommits>,
    expires: Instant,
}

type Slot = Arc<AsyncMutex<Option<Cached>>>;

#[derive(Debug)]
pub(super) struct RecentWalks {
    slots: Mutex<VecDeque<(WalkKey, Slot)>>,
    permits: Arc<Semaphore>,
    capacity: usize,
    max_bytes: usize,
    ttl: Duration,
}

impl Default for RecentWalks {
    fn default() -> Self {
        Self::new(32, 4, 512 * 1024, Duration::from_secs(30))
    }
}

impl RecentWalks {
    fn new(capacity: usize, workers: usize, max_bytes: usize, ttl: Duration) -> Self {
        assert!(capacity > workers && workers > 0);
        Self { slots: Mutex::default(), permits: Arc::new(Semaphore::new(workers)), capacity, max_bytes, ttl }
    }

    pub async fn run<F, W>(&self, key: WalkKey, compute: F) -> Result<Arc<LastCommits>, ServiceError>
    where
        F: FnOnce() -> W + Send + 'static,
        W: Future<Output = Result<LastCommits, ServiceError>> + Send + 'static,
    {
        // Warm hits do not queue behind unrelated expensive work.
        {
            let slots = self.slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((_, slot)) = slots.iter().find(|(k, _)| k == &key)
                && let Ok(value) = slot.try_lock()
                && let Some(cached) = value.as_ref().filter(|cached| cached.expires > Instant::now())
            {
                return Ok(Arc::clone(&cached.value));
            }
        }
        let permit = Arc::clone(&self.permits).acquire_owned().await.map_err(ServiceError::internal)?;
        let key_bytes = key.commit.capacity()
            + key.path.capacity()
            + key.names.capacity() * std::mem::size_of::<String>()
            + key.names.iter().map(String::capacity).sum::<usize>();
        if key_bytes >= self.max_bytes {
            return tokio::spawn(async move {
                let _permit = permit;
                compute().await.map(Arc::new)
            })
            .await
            .map_err(ServiceError::internal)?;
        }
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(index) = slots.iter().position(|(k, _)| k == &key) {
                let entry = slots.remove(index).expect("known slot");
                let slot = Arc::clone(&entry.1);
                slots.push_back(entry);
                slot
            } else {
                if slots.len() == self.capacity {
                    // At most `workers` slots have an external owner, so a cached-only
                    // slot exists. Never evict an active slot and start duplicate work.
                    let index = slots.iter().position(|(_, s)| Arc::strong_count(s) == 1).expect("capacity exceeds workers");
                    slots.remove(index);
                }
                let slot = Arc::new(AsyncMutex::new(None));
                slots.push_back((key, Arc::clone(&slot)));
                slot
            }
        };
        let max_bytes = self.max_bytes - key_bytes;
        let ttl = self.ttl;
        tokio::spawn(async move {
            let _permit = permit;
            let mut cached = slot.lock().await;
            if let Some(value) = cached.as_ref().filter(|value| value.expires > Instant::now()) {
                return Ok(Arc::clone(&value.value));
            }
            *cached = None;
            let value = Arc::new(compute().await?);
            if result_bytes(&value) <= max_bytes {
                *cached = Some(Cached { value: Arc::clone(&value), expires: Instant::now() + ttl });
            }
            Ok(value)
        })
        .await
        .map_err(ServiceError::internal)?
    }
}

fn result_bytes(value: &LastCommits) -> usize {
    value.commits.iter().fold(0usize, |bytes, (name, commit)| {
        let strings = [&commit.id, &commit.short_id, &commit.summary, &commit.author_name, &commit.author_email];
        bytes
            .saturating_add(name.capacity())
            .saturating_add(std::mem::size_of_val(commit) + 128)
            .saturating_add(strings.iter().map(|s| s.capacity()).sum::<usize>())
            .saturating_add(commit.body.as_ref().map_or(0, String::capacity))
            .saturating_add(commit.parent_ids.iter().map(|s| s.capacity() + std::mem::size_of::<String>()).sum::<usize>())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn key(path: &str) -> WalkKey {
        WalkKey { repository: rbit_core::RepoId(1), commit: "a".into(), path: path.into(), names: vec!["file".into()] }
    }

    #[tokio::test]
    async fn concurrent_partial_results_are_shared_and_expire() {
        let cache = Arc::new(RecentWalks::new(3, 2, 1024, Duration::from_secs(30)));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            tasks.push(tokio::spawn(async move {
                cache
                    .run(key(""), move || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok(LastCommits { timed_out: true, ..Default::default() })
                    })
                    .await
                    .unwrap()
            }));
        }
        for task in tasks {
            assert!(task.await.unwrap().timed_out);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let slot = Arc::clone(&cache.slots.lock().unwrap()[0].1);
        slot.lock().await.as_mut().unwrap().expires = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        let result = cache.run(key(""), || async { Ok(LastCommits::default()) }).await.unwrap();
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn cancellation_keeps_work_admitted_and_cache_population_alive() {
        let cache = Arc::new(RecentWalks::new(2, 1, 1024, Duration::from_secs(30)));
        let (started, wait) = tokio::sync::oneshot::channel();
        let (finish, finished) = tokio::sync::oneshot::channel();
        let worker = Arc::clone(&cache);
        let request = tokio::spawn(async move {
            worker
                .run(key(""), || async {
                    started.send(()).unwrap();
                    finished.await.unwrap();
                    Ok(LastCommits::default())
                })
                .await
        });
        wait.await.unwrap();
        request.abort();
        assert_eq!(cache.permits.available_permits(), 0);
        finish.send(()).unwrap();
        cache.run(key(""), || async { panic!("cancelled request still populates cache") }).await.unwrap();
    }

    #[tokio::test]
    async fn failures_retry_and_distinct_keys_remain_bounded() {
        let cache = RecentWalks::new(3, 1, 1024, Duration::from_secs(30));
        assert!(cache.run(key(""), || async { Err(ServiceError::NotFound) }).await.is_err());
        cache.run(key(""), || async { Ok(LastCommits::default()) }).await.unwrap();
        for n in 0..10 {
            cache.run(key(&n.to_string()), || async { Ok(LastCommits::default()) }).await.unwrap();
        }
        assert_eq!(cache.slots.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn entry_names_are_part_of_the_key_and_oversized_entries_are_not_retained() {
        let cache = RecentWalks::new(3, 1, 1024, Duration::from_secs(30));
        cache.run(key(""), || async { Ok(LastCommits { timed_out: true, ..Default::default() }) }).await.unwrap();
        let mut other = key("");
        other.names = vec!["other-page".into()];
        assert!(!cache.run(other, || async { Ok(LastCommits::default()) }).await.unwrap().timed_out);
        let tiny = RecentWalks::new(3, 1, 1, Duration::from_secs(30));
        tiny.run(key(""), || async { Ok(LastCommits::default()) }).await.unwrap();
        assert!(tiny.slots.lock().unwrap().is_empty());
    }
}
