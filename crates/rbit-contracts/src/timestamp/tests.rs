use super::*;

#[test]
fn relative_formatting() {
    let now = Timestamp(1_000_000_000);
    assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 5), now), "just now");
    assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 60), now), "1 minute ago");
    assert_eq!(Timestamp::relative_between(Timestamp(now.0 - 7200), now), "2 hours ago");
}

#[test]
fn until_formatting() {
    let now = Timestamp(1_000_000_000);
    let until = |seconds: i64| Timestamp::until_between(Timestamp(now.0 + seconds), now);
    assert_eq!(until(-5), "in under a minute", "a time just passed is not in the past tense");
    assert_eq!(until(30), "in under a minute");
    assert_eq!(until(MINUTE), "in 1 minute");
    assert_eq!(until(HOUR - 1), "in 1 hour", "a fresh one-hour token");
    assert_eq!(until(HOUR - 20), "in 1 hour", "rounding up never says 60 minutes");
    assert_eq!(until(DAY - HOUR), "in 23 hours");
    assert_eq!(until(DAY - 20 * MINUTE), "in 1 day");
    assert_eq!(until(90 * DAY - HOUR), "in 3 months", "a 90-day expiry seen an hour later");
    assert_eq!(until(400 * DAY), format!("on {}", Timestamp(now.0 + 400 * DAY).date()));
}
#[test]
fn signed_tuple_encoding_matches_json_and_existing_postcard_i64_vectors() {
    for value in [i64::MIN, -64, -1, 0, 1, 64, 1_000_000_000, i64::MAX] {
        let timestamp = Timestamp(value);
        assert_eq!(timestamp.seconds(), value);
        assert_eq!(serde_json::to_string(&timestamp).unwrap(), value.to_string());
        assert_eq!(serde_json::from_str::<Timestamp>(&value.to_string()).unwrap(), timestamp);
        let bytes = postcard::to_allocvec(&timestamp).unwrap();
        assert_eq!(bytes, postcard::to_allocvec(&value).unwrap());
        assert_eq!(postcard::from_bytes::<Timestamp>(&bytes).unwrap(), timestamp);
    }
    for (value, expected) in [(0, vec![0]), (-1, vec![1]), (1, vec![2]), (64, vec![128, 1])] {
        assert_eq!(postcard::to_allocvec(&Timestamp(value)).unwrap(), expected);
    }
}

#[test]
fn machine_date_and_out_of_range_fallback_remain_compatible() {
    assert_eq!(Timestamp(0).iso8601(), "1970-01-01T00:00:00Z");
    assert_eq!(Timestamp(0).date(), "Jan 1, 1970");
    assert_eq!(Timestamp(-1).iso8601(), "1969-12-31T23:59:59Z");
    for invalid in [i64::MIN, i64::MAX] {
        assert_eq!(Timestamp(invalid).iso8601(), Timestamp(0).iso8601());
        assert_eq!(Timestamp(invalid).date(), Timestamp(0).date());
    }
}
