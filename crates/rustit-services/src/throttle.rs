//! Rate limiting for authentication and other abuse-prone endpoints.
//!
//! Milestone 1 workstream S owns this file. The API below is a contract used by other
//! workstreams (email recovery, SSH); keep the signatures stable.
//!
//! Every `(scope, key)` pair has a budget that refills steadily up to a burst size. Each
//! [`ThrottleService::check`] spends one unit, each [`ThrottleService::record_failure`]
//! spends extra (so guessing exhausts the budget much faster than ordinary use), and
//! [`ThrottleService::reset`] restores the full budget after a success.
//!
//! The implementation is the generic cell rate algorithm: a bucket is a single
//! "theoretical arrival time" instant, so state is one `Instant` per key. State lives in
//! memory, which is right for a single binary: a restart forgets it, but an attacker
//! cannot restart the server.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use crate::{ServiceContext, ServiceError};

/// What is being throttled. Each scope has its own limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThrottleScope {
    /// Password sign-in attempts (keyed by client address and by login name).
    SignIn,
    /// Account creation (keyed by client address).
    SignUp,
    /// Second-factor code attempts (keyed by account).
    TwoFactor,
    /// Password-reset and verification emails (keyed by client address and by address).
    EmailRequest,
    /// HTTP Basic credentials on the Git endpoints (keyed by client address and login).
    GitAuth,
    /// SSH authentication attempts (keyed by client address).
    SshAuth,
    /// Access tokens presented to the REST API (keyed by client address); invalid ones cost extra.
    ApiAuth,
    /// Authenticated REST API requests (keyed by account), so one script cannot monopolise the instance.
    ApiRequest,
}

/// A budget: `burst` units, refilled one per `refill`; failures cost `failure_cost` more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Limit {
    burst: u32,
    refill: Duration,
    failure_cost: u32,
}

impl Limit {
    const fn new(burst: u32, refill_seconds: u64, failure_cost: u32) -> Self {
        Self { burst, refill: Duration::from_secs(refill_seconds), failure_cost }
    }

    /// The longest a bucket can be "in debt": a completely empty budget.
    fn capacity(self) -> Duration {
        self.refill.saturating_mul(self.burst)
    }
}

/// A budget is tracked per bucket kind and key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Bucket {
    Scope(ThrottleScope),
    /// A scope keyed by client address: many people can share one (offices, carrier NAT,
    /// a proxy that is not listed as trusted), so it gets [`CLIENT_FACTOR`] times the budget.
    Client(ThrottleScope),
    /// Any state-changing request to the web UI, keyed by client address.
    UnsafeRequest,
}

/// How much larger per-address budgets are than per-account ones.
const CLIENT_FACTOR: u32 = 5;

impl Bucket {
    fn limit(self) -> Limit {
        match self {
            Self::Client(scope) => {
                let limit = Self::Scope(scope).limit();
                Limit { burst: limit.burst * CLIENT_FACTOR, refill: limit.refill / CLIENT_FACTOR, failure_cost: limit.failure_cost }
            }
            // ~7 wrong passwords in quick succession, then one a minute.
            Self::Scope(ThrottleScope::SignIn) => Limit::new(20, 20, 2),
            // Five accounts per address, then one every ten minutes.
            Self::Scope(ThrottleScope::SignUp) => Limit::new(5, 600, 0),
            // ~3 wrong codes quickly, then one every 90 seconds: the 10⁶ code space stays out of reach.
            Self::Scope(ThrottleScope::TwoFactor) => Limit::new(10, 30, 2),
            // Three emails, then one every ten minutes.
            Self::Scope(ThrottleScope::EmailRequest) => Limit::new(3, 600, 0),
            // A clone or push is 2–3 authenticated requests; wrong passwords cost 11 requests' worth.
            // API tokens get the same budget: a valid token costs one request, an invalid one eleven.
            Self::Scope(ThrottleScope::GitAuth | ThrottleScope::ApiAuth) => Limit::new(30, 4, 10),
            Self::Scope(ThrottleScope::SshAuth) => Limit::new(20, 3, 5),
            // 300 requests at once, then one per second (3,600 an hour) per account.
            Self::Scope(ThrottleScope::ApiRequest) => Limit::new(300, 1, 0),
            // Generous: a person clicking around never notices, a script hammering forms does.
            Self::UnsafeRequest => Limit::new(60, 1, 0),
        }
    }
}

/// Keys longer than this are truncated so a hostile client cannot grow memory per entry.
const MAX_KEY_LEN: usize = 128;
/// Once this many buckets exist, fully refilled ones are dropped on the next insert.
const PRUNE_THRESHOLD: usize = 10_000;

type Buckets = HashMap<(Bucket, String), Instant>;

#[derive(Debug, Clone)]
pub struct ThrottleService {
    enabled: bool,
    /// Theoretical arrival time per bucket: the budget is full once `now` reaches it.
    buckets: Arc<Mutex<Buckets>>,
}

impl ThrottleService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let ServiceContext { config, .. } = ctx;
        Self::with_enabled(config.security.rate_limit)
    }

    fn with_enabled(enabled: bool) -> Self {
        Self { enabled, buckets: Arc::default() }
    }

    /// Fails with [`ServiceError::RateLimited`] when `key` has exhausted its budget in `scope`.
    /// Call before doing the expensive or sensitive work.
    pub fn check(&self, scope: ThrottleScope, key: &str) -> Result<(), ServiceError> {
        self.spend(Bucket::Scope(scope), key, Instant::now())
    }

    /// Records a failed attempt (wrong password, bad code), which consumes extra budget.
    pub fn record_failure(&self, scope: ThrottleScope, key: &str) {
        self.penalise(Bucket::Scope(scope), key, Instant::now());
    }

    /// Clears the failure history after a success (e.g. a correct password).
    pub fn reset(&self, scope: ThrottleScope, key: &str) {
        if self.enabled {
            self.lock().remove(&(Bucket::Scope(scope), bucket_key(key)));
        }
    }

    /// Like [`Self::check`], for a key that is a client address: the budget is larger,
    /// because one address can stand for many people.
    pub fn check_client(&self, scope: ThrottleScope, address: &str) -> Result<(), ServiceError> {
        self.spend(Bucket::Client(scope), address, Instant::now())
    }

    /// Like [`Self::record_failure`], for a client address.
    pub fn record_client_failure(&self, scope: ThrottleScope, address: &str) {
        self.penalise(Bucket::Client(scope), address, Instant::now());
    }

    /// Like [`Self::reset`], for a client address.
    pub fn reset_client(&self, scope: ThrottleScope, address: &str) {
        if self.enabled {
            self.lock().remove(&(Bucket::Client(scope), bucket_key(address)));
        }
    }

    /// Per-client budget for state-changing web requests (form posts), applied by the
    /// web layer's rate-limit middleware.
    pub fn check_unsafe_request(&self, client: &str) -> Result<(), ServiceError> {
        self.spend(Bucket::UnsafeRequest, client, Instant::now())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Buckets> {
        // The map is always left consistent, so a panic elsewhere never poisons it meaningfully.
        self.buckets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Spends one unit at `now`, or reports how long until one is available.
    fn spend(&self, bucket: Bucket, key: &str, now: Instant) -> Result<(), ServiceError> {
        if !self.enabled {
            return Ok(());
        }
        let limit = bucket.limit();
        let mut buckets = self.lock();
        let key = (bucket, bucket_key(key));
        let tat = buckets.get(&key).copied().unwrap_or(now).max(now);
        let next = tat + limit.refill;
        let debt = next.saturating_duration_since(now);
        if debt > limit.capacity() {
            let wait = debt.saturating_sub(limit.capacity());
            return Err(ServiceError::RateLimited { retry_after_seconds: whole_seconds(wait) });
        }
        if buckets.len() >= PRUNE_THRESHOLD && !buckets.contains_key(&key) {
            buckets.retain(|_, tat| *tat > now);
        }
        buckets.insert(key, next);
        Ok(())
    }

    /// Spends the failure surcharge at `now`, never beyond an empty budget.
    fn penalise(&self, bucket: Bucket, key: &str, now: Instant) {
        let limit = bucket.limit();
        if !self.enabled || limit.failure_cost == 0 {
            return;
        }
        let mut buckets = self.lock();
        let tat = buckets.entry((bucket, bucket_key(key))).or_insert(now);
        let charged = (*tat).max(now) + limit.refill.saturating_mul(limit.failure_cost);
        *tat = charged.min(now + limit.capacity());
    }
}

/// Keys are compared case-insensitively (logins are) and length-capped.
fn bucket_key(key: &str) -> String {
    let end = key.char_indices().nth(MAX_KEY_LEN).map_or(key.len(), |(index, _)| index);
    key[..end].to_lowercase()
}

/// Rounds up, so clients never retry a moment too early; at least one second.
fn whole_seconds(duration: Duration) -> u64 {
    let seconds = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    seconds.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGN_IN: Bucket = Bucket::Scope(ThrottleScope::SignIn);

    fn retry_after(result: Result<(), ServiceError>) -> Option<u64> {
        match result {
            Ok(()) => None,
            Err(ServiceError::RateLimited { retry_after_seconds }) => Some(retry_after_seconds),
            Err(other) => panic!("unexpected error {other}"),
        }
    }

    #[test]
    fn allows_a_burst_then_refills_steadily() {
        let throttle = ThrottleService::with_enabled(true);
        let start = Instant::now();
        let limit = SIGN_IN.limit();
        for _ in 0..limit.burst {
            throttle.spend(SIGN_IN, "203.0.113.7", start).unwrap();
        }
        assert_eq!(retry_after(throttle.spend(SIGN_IN, "203.0.113.7", start)), Some(limit.refill.as_secs()));
        // Other keys are unaffected.
        throttle.spend(SIGN_IN, "198.51.100.1", start).unwrap();
        // One refill period later, exactly one more attempt is allowed.
        let later = start + limit.refill;
        throttle.spend(SIGN_IN, "203.0.113.7", later).unwrap();
        assert!(retry_after(throttle.spend(SIGN_IN, "203.0.113.7", later)).is_some());
    }

    #[test]
    fn failures_cost_extra_and_success_resets() {
        let throttle = ThrottleService::with_enabled(true);
        let now = Instant::now();
        let limit = SIGN_IN.limit();
        let per_failure = 1 + limit.failure_cost;
        let mut allowed = 0;
        while throttle.spend(SIGN_IN, "alice", now).is_ok() {
            throttle.penalise(SIGN_IN, "alice", now);
            allowed += 1;
        }
        assert_eq!(allowed, limit.burst.div_ceil(per_failure));
        assert!(allowed < limit.burst, "guessing runs out faster than ordinary use");

        throttle.reset(ThrottleScope::SignIn, "ALICE");
        throttle.check(ThrottleScope::SignIn, "alice").unwrap();
    }

    #[test]
    fn penalties_never_exceed_an_empty_budget() {
        let throttle = ThrottleService::with_enabled(true);
        let now = Instant::now();
        let limit = SIGN_IN.limit();
        for _ in 0..100 {
            throttle.penalise(SIGN_IN, "mallory", now);
        }
        assert_eq!(retry_after(throttle.spend(SIGN_IN, "mallory", now)), Some(limit.refill.as_secs()));
        throttle.spend(SIGN_IN, "mallory", now + limit.refill).unwrap();
    }

    #[test]
    fn scopes_are_independent_and_free_failures_are_free() {
        let throttle = ThrottleService::with_enabled(true);
        let now = Instant::now();
        let sign_up = Bucket::Scope(ThrottleScope::SignUp);
        for _ in 0..sign_up.limit().burst {
            throttle.spend(sign_up, "203.0.113.7", now).unwrap();
            throttle.penalise(sign_up, "203.0.113.7", now);
        }
        assert!(throttle.spend(sign_up, "203.0.113.7", now).is_err());
        throttle.spend(SIGN_IN, "203.0.113.7", now).unwrap();
        throttle.spend(Bucket::UnsafeRequest, "203.0.113.7", now).unwrap();
    }

    #[test]
    fn client_addresses_get_a_larger_budget() {
        let throttle = ThrottleService::with_enabled(true);
        let now = Instant::now();
        let client = Bucket::Client(ThrottleScope::SignIn);
        assert_eq!(client.limit().burst, SIGN_IN.limit().burst * CLIENT_FACTOR);
        for _ in 0..client.limit().burst {
            throttle.spend(client, "203.0.113.7", now).unwrap();
        }
        assert_eq!(retry_after(throttle.spend(client, "203.0.113.7", now)), Some(4));
        throttle.check(ThrottleScope::SignIn, "203.0.113.7").unwrap();
        throttle.reset_client(ThrottleScope::SignIn, "203.0.113.7");
        throttle.check_client(ThrottleScope::SignIn, "203.0.113.7").unwrap();
    }

    #[test]
    fn disabled_throttle_allows_everything() {
        let throttle = ThrottleService::with_enabled(false);
        for _ in 0..1000 {
            throttle.check(ThrottleScope::TwoFactor, "1").unwrap();
            throttle.record_failure(ThrottleScope::TwoFactor, "1");
        }
    }

    #[test]
    fn prunes_refilled_buckets() {
        let throttle = ThrottleService::with_enabled(true);
        let now = Instant::now();
        for i in 0..PRUNE_THRESHOLD {
            throttle.spend(Bucket::UnsafeRequest, &i.to_string(), now).unwrap();
        }
        assert_eq!(throttle.lock().len(), PRUNE_THRESHOLD);
        throttle.spend(Bucket::UnsafeRequest, "new", now + Duration::from_secs(5)).unwrap();
        assert_eq!(throttle.lock().len(), 1);
    }

    #[test]
    fn keys_are_normalised() {
        assert_eq!(bucket_key("Alice"), "alice");
        assert_eq!(bucket_key(&"é".repeat(500)).chars().count(), MAX_KEY_LEN);
        assert_eq!(whole_seconds(Duration::from_millis(1)), 1);
        assert_eq!(whole_seconds(Duration::from_millis(20_001)), 21);
        assert_eq!(whole_seconds(Duration::ZERO), 1);
    }
}
