use super::*;

#[test]
fn backoff_follows_the_schedule_with_bounded_jitter() {
    let expected = [10, 60, 600, 3_600, 21_600, 21_600, 21_600];
    for (failures, seconds) in (1..).zip(expected) {
        assert_eq!(retry_delay(failures, 0.5), Duration::from_secs(seconds), "after {failures} failures");
        let low = retry_delay(failures, 0.0);
        let high = retry_delay(failures, 0.999_999);
        assert_eq!(low, Duration::from_secs(seconds).mul_f64(0.8));
        assert!(high < Duration::from_secs(seconds).mul_f64(1.2) && high > Duration::from_secs(seconds));
    }
    assert_eq!(retry_delay(0, 0.5), Duration::from_secs(10), "zero failures behaves like one");
    assert_eq!(retry_delay(1, 7.0), retry_delay(1, 1.0), "out-of-range randomness is clamped");
}

#[test]
fn attempts_succeed_fail_or_retry() {
    let never = || -> Timestamp { panic!("the clock is read only for retries") };
    let no_random = || -> f64 { panic!("randomness is read only for retries") };
    assert_eq!(next_step(true, true, 9, 1, never, no_random), NextStep::Succeeded);
    assert_eq!(next_step(false, true, 0, 5, never, no_random), NextStep::Failed, "a refused target fails at once");
    assert_eq!(next_step(false, false, 4, 5, never, no_random), NextStep::Failed, "the fifth failure is the last");
    assert_eq!(next_step(false, false, 0, 0, never, no_random), NextStep::Failed, "zero attempts behaves like one");
    assert_eq!(next_step(false, false, 1, 5, || Timestamp(100), || 0.5), NextStep::Retry(Timestamp(160)));
    assert_eq!(
        [NextStep::Succeeded, NextStep::Retry(Timestamp(0)), NextStep::Failed].map(NextStep::label),
        ["succeeded", "retrying", "failed"]
    );
}

#[test]
fn secrets_are_bounded() {
    assert_eq!(check_secret(""), Ok(None));
    assert_eq!(check_secret("s3cret value").unwrap().as_deref(), Some("s3cret value"));
    assert_eq!(check_secret(&"x".repeat(MAX_SECRET_LEN + 1)), Err(SecretError::TooLong));
    assert_eq!(check_secret("line\nbreak"), Err(SecretError::ControlCharacter));
    assert_eq!(SecretError::TooLong.to_string(), "secret must be at most 256 bytes");
    assert_eq!(SecretError::ControlCharacter.to_string(), "secret must not contain control characters");
    assert_eq!(retention_cutoff(Timestamp(10 * 86_400), 14), Timestamp(-4 * 86_400));
}

#[test]
fn hook_limits_and_log_pages_keep_their_arithmetic() {
    assert_eq!(hook_limit_error(19, true), None);
    assert_eq!(hook_limit_error(20, true).as_deref(), Some("a repository can have at most 20 webhooks; delete one first"));
    // The original wording, article included, is kept as released.
    assert_eq!(hook_limit_error(21, false).as_deref(), Some("a organization can have at most 20 webhooks; delete one first"));
    assert_eq!(delivery_page(0, 0), (1, 0));
    assert_eq!(delivery_page(3, 30), (30, 60));
    assert_eq!(delivery_page(u32::MAX, u32::MAX), (u32::MAX, u32::MAX));
}

#[test]
fn event_lists_are_trimmed_sorted_deduplicated_and_gated() {
    let catalog = EventCatalog {
        parse: |name: &str| ["ping", "push", "release", "workflow_run"].iter().position(|known| *known == name),
        ping: |event: usize| event == 0,
        needs_actions: |event: usize| event == 3,
    };
    let names = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
    assert_eq!(subscribed_events(&names(&["release", " push ", "push", "ping"]), &catalog, false), Ok(vec![1, 2]));
    assert_eq!(subscribed_events(&names(&["ping"]), &catalog, false), Err(EventsError::Empty));
    assert_eq!(subscribed_events(&names(&["workflow_run"]), &catalog, true), Ok(vec![3]));
    let gated = subscribed_events(&names(&["workflow_run"]), &catalog, false).unwrap_err();
    assert_eq!(gated.to_string(), "\"workflow_run\" needs Actions, which this instance does not have");
    let unknown = subscribed_events(&names(&["discussion"]), &catalog, false).unwrap_err();
    assert_eq!(unknown.to_string(), "unknown event \"discussion\"; choose from push, repository, release and fork");
    assert_eq!(EventsError::Empty.to_string(), "choose at least one event");
}
