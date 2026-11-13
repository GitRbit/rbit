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

// Actions: S.
#[test]
fn runner_authentication_spends_only_on_failure() {
    let throttle = ThrottleService::with_enabled(true);
    let now = Instant::now();
    let bucket = Bucket::Client(ThrottleScope::RunnerAuth);
    for _ in 0..1000 {
        throttle.peek(bucket, "203.0.113.7", now).unwrap();
    }
    let mut failures = 0;
    while throttle.peek(bucket, "203.0.113.7", now).is_ok() {
        throttle.penalise(bucket, "203.0.113.7", now);
        failures += 1;
    }
    assert_eq!(failures, bucket.limit().burst, "every failure costs one unit");
    assert!(retry_after(throttle.peek(bucket, "203.0.113.7", now)).is_some());
    throttle.peek(bucket, "198.51.100.1", now).unwrap();
    throttle.peek(bucket, "203.0.113.7", now + bucket.limit().refill).unwrap();
    assert!(ThrottleService::with_enabled(false).check_client_failures(ThrottleScope::RunnerAuth, "x").is_ok());
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

#[test]
fn an_ipv6_network_is_one_client() {
    assert_eq!(bucket_key("2001:db8:1:2:aaaa::1"), bucket_key("2001:db8:1:2:ffff::9"), "one /64");
    assert_ne!(bucket_key("2001:db8:1:2::1"), bucket_key("2001:db8:1:3::1"));
    assert_eq!(bucket_key("203.0.113.7"), "203.0.113.7");
    assert_eq!(bucket_key("login:alice"), "login:alice");
    let throttle = ThrottleService::with_enabled(true);
    let now = Instant::now();
    let bucket = Bucket::Client(ThrottleScope::SignIn);
    // Spending from every address of the /64 drains one budget.
    let spent = (0..10_000).take_while(|n| throttle.spend(bucket, &format!("2001:db8::{n:x}"), now).is_ok()).count();
    assert!(spent < 10_000, "{spent}");
}
