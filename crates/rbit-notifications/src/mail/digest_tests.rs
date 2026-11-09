use super::*;

/// 2026-10-19 00:00 UTC, a Monday.
const MONDAY: i64 = 1_792_368_000;

#[test]
fn the_reference_monday_is_a_monday() {
    // 2026-10-19 is 20 745 days after the epoch; (20 745 + 3) % 7 == 0 means Monday.
    assert_eq!(MONDAY % DAY, 0);
    assert_eq!((MONDAY / DAY + 3) % 7, 0);
    assert_eq!((FIRST_MONDAY / DAY + 3) % 7, 0);
}

#[test]
fn daily_periods_start_at_the_digest_hour() {
    let at = |seconds: i64| period_start(Cadence::Daily, 8, Timestamp(MONDAY + seconds));
    assert_eq!(at(8 * HOUR), Timestamp(MONDAY + 8 * HOUR), "the boundary itself starts the period");
    assert_eq!(at(8 * HOUR - 1), Timestamp(MONDAY - DAY + 8 * HOUR), "a second before belongs to the day before");
    assert_eq!(at(23 * HOUR), Timestamp(MONDAY + 8 * HOUR));
    assert_eq!(at(DAY + 7 * HOUR), Timestamp(MONDAY + 8 * HOUR));
    assert_eq!(period_start(Cadence::Daily, 0, Timestamp(MONDAY + 1)), Timestamp(MONDAY));
    assert_eq!(period_start(Cadence::Immediate, 8, Timestamp(MONDAY + 9 * HOUR)), Timestamp(MONDAY + 8 * HOUR));
    assert_eq!(period_start(Cadence::Daily, 99, Timestamp(MONDAY + 23 * HOUR)), Timestamp(MONDAY + 23 * HOUR), "hours are clamped");
}

#[test]
fn weekly_periods_start_on_mondays() {
    let at = |seconds: i64| period_start(Cadence::Weekly, 8, Timestamp(MONDAY + seconds));
    assert_eq!(at(8 * HOUR), Timestamp(MONDAY + 8 * HOUR));
    assert_eq!(at(6 * DAY + 23 * HOUR), Timestamp(MONDAY + 8 * HOUR), "Sunday night is still the same week");
    assert_eq!(at(7 * DAY + 8 * HOUR), Timestamp(MONDAY + WEEK + 8 * HOUR));
    assert_eq!(at(8 * HOUR - 1), Timestamp(MONDAY - WEEK + 8 * HOUR), "Monday before the hour belongs to last week");
    assert_eq!(period_start(Cadence::Weekly, 0, Timestamp(0)), Timestamp(FIRST_MONDAY - WEEK), "before the first Monday");
}

#[test]
fn pending_notifications_are_kept_for_two_weeks() {
    let now = Timestamp(MONDAY);
    assert_eq!(prune_cutoff(Timestamp(MONDAY - DAY), now), Timestamp(MONDAY - 2 * WEEK));
    assert_eq!(prune_cutoff(Timestamp(MONDAY - 90 * DAY), now), Timestamp(MONDAY - 90 * DAY));
}
