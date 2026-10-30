//! Concurrency limits for long-running work clients start: Git transport processes and
//! archive downloads.
//!
//! A [`ConcurrencyLimiter`] caps how many run at once across the instance and, for callers
//! that know the client's address, how many one IP address may hold. Admission never
//! waits: a refused caller answers at once ([`Refused`]) instead of queueing unboundedly.
//! The returned [`ConcurrencySlot`] is the place within both limits; dropping it frees it.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex, PoisonError},
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// What a refused caller suggests as `Retry-After`, in seconds.
pub const RETRY_AFTER_SECONDS: u64 = 10;

/// Why a slot was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// Every slot of the instance is in use.
    #[error("the server is busy; try again shortly")]
    Busy,
    /// This client already holds as many slots as one address may.
    #[error("too many concurrent requests from this address; try again shortly")]
    TooManyFromClient,
}

/// Caps concurrent work instance-wide and per client address. Cheap to clone; clones
/// share the limits.
#[derive(Debug, Clone)]
pub struct ConcurrencyLimiter {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    total: Arc<Semaphore>,
    per_ip: Mutex<HashMap<IpAddr, usize>>,
    max_per_ip: usize,
}

impl ConcurrencyLimiter {
    /// At most `total` at once, and `max_per_ip` from one address (both at least one).
    pub fn new(total: usize, max_per_ip: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                total: Arc::new(Semaphore::new(total.max(1))),
                per_ip: Mutex::new(HashMap::new()),
                max_per_ip: max_per_ip.max(1),
            }),
        }
    }

    /// Takes a slot for a client at `ip`, or only an instance-wide one without an address
    /// (callers that cap clients themselves, like the SSH server's per-address
    /// connection limit).
    pub fn try_acquire(&self, ip: Option<IpAddr>) -> Result<ConcurrencySlot, Refused> {
        let ip = ip.map(|ip| ip.to_canonical());
        let mut per_ip = self.inner.per_ip.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(ip) = ip
            && per_ip.get(&ip).is_some_and(|count| *count >= self.inner.max_per_ip)
        {
            return Err(Refused::TooManyFromClient);
        }
        let permit = Arc::clone(&self.inner.total).try_acquire_owned().map_err(|_| Refused::Busy)?;
        if let Some(ip) = ip {
            *per_ip.entry(ip).or_default() += 1;
        }
        Ok(ConcurrencySlot { inner: Arc::clone(&self.inner), ip, _permit: permit })
    }

    /// Slots free instance-wide.
    pub fn available(&self) -> usize {
        self.inner.total.available_permits()
    }
}

/// A place within a [`ConcurrencyLimiter`]'s limits; released when dropped.
#[derive(Debug)]
pub struct ConcurrencySlot {
    inner: Arc<Inner>,
    ip: Option<IpAddr>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for ConcurrencySlot {
    fn drop(&mut self) {
        let Some(ip) = self.ip else { return };
        let mut per_ip = self.inner.per_ip.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = per_ip.get_mut(&ip) {
            *count -= 1;
            if *count == 0 {
                per_ip.remove(&ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    const ALICE: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
    const BOB: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));

    #[test]
    fn caps_each_address_and_the_instance() {
        let limiter = ConcurrencyLimiter::new(3, 2);
        let first = limiter.try_acquire(Some(ALICE)).unwrap();
        let _second = limiter.try_acquire(Some(ALICE)).unwrap();
        assert_eq!(limiter.try_acquire(Some(ALICE)).unwrap_err(), Refused::TooManyFromClient);
        let _bob = limiter.try_acquire(Some(BOB)).unwrap();
        assert_eq!(limiter.try_acquire(Some(BOB)).unwrap_err(), Refused::Busy);
        assert_eq!(limiter.try_acquire(None).unwrap_err(), Refused::Busy);
        assert_eq!(limiter.available(), 0);

        drop(first);
        assert_eq!(limiter.available(), 1);
        let _again = limiter.try_acquire(Some(ALICE)).unwrap();
    }

    #[test]
    fn a_busy_instance_does_not_count_against_the_client() {
        let limiter = ConcurrencyLimiter::new(1, 1);
        let held = limiter.try_acquire(None).unwrap();
        assert_eq!(limiter.try_acquire(Some(ALICE)).unwrap_err(), Refused::Busy);
        drop(held);
        let _slot = limiter.try_acquire(Some(ALICE)).unwrap();
    }

    #[test]
    fn mapped_ipv4_addresses_are_the_same_client() {
        let limiter = ConcurrencyLimiter::new(4, 1);
        let _v4 = limiter.try_acquire(Some(ALICE)).unwrap();
        let mapped = IpAddr::V6(Ipv4Addr::new(192, 0, 2, 1).to_ipv6_mapped());
        assert_eq!(limiter.try_acquire(Some(mapped)).unwrap_err(), Refused::TooManyFromClient);
        let _other = limiter.try_acquire(Some(IpAddr::V6(Ipv6Addr::LOCALHOST))).unwrap();
    }

    #[test]
    fn released_addresses_are_forgotten() {
        let limiter = ConcurrencyLimiter::new(4, 1);
        drop(limiter.try_acquire(Some(ALICE)).unwrap());
        assert!(limiter.inner.per_ip.lock().unwrap().is_empty());
    }
}
