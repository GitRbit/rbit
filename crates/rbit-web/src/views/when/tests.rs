use std::time::Duration;

use rbit_core::Timestamp;
use rbit_localization::Locale;

use super::*;

const HOUR: i64 = 3_600;
const DAY: i64 = 24 * HOUR;
/// 2026-11-08T12:00:00Z.
const NOW: Timestamp = Timestamp(1_794_139_200);

#[test]
fn english_matches_the_core_formatter() {
    // The core formatter measures from the real clock; spans far from a unit boundary
    // read the same whichever second the test runs in.
    let now = Timestamp::now();
    for ago in [0, 5 * 60 + 30, 3 * HOUR + 600, 4 * DAY + 600, 70 * DAY, 800 * DAY] {
        let then = Timestamp(now.seconds() - ago);
        assert_eq!(relative_at(then, now, Locale::English).as_str(), then.relative(), "{ago} seconds ago");
    }
    for ahead in [30, 90 * DAY - HOUR, 2 * HOUR + 60, 500 * DAY] {
        let then = Timestamp(now.seconds() + ahead);
        assert_eq!(until_at(then, now, Locale::English).as_str(), then.until(), "{ahead} seconds ahead");
    }
    assert_eq!(date(NOW, Locale::English).as_str(), NOW.date());
}

#[test]
fn relative_times_read_in_the_page_language() {
    let three_hours = Timestamp(NOW.seconds() - 3 * HOUR);
    let cases = [
        (Locale::English, "3 hours ago"),
        (Locale::German, "vor 3 Stunden"),
        (Locale::Spanish, "hace 3 horas"),
        (Locale::French, "il y a 3 heures"),
        (Locale::PortugueseBrazil, "há 3 horas"),
        (Locale::Russian, "3 часа назад"),
        (Locale::Japanese, "3 時間前"),
        (Locale::ChineseSimplified, "3小时前"),
        (Locale::Korean, "3시간 전"),
    ];
    for (locale, expected) in cases {
        let text = relative_at(three_hours, NOW, locale);
        assert_eq!(text.as_str(), expected);
        assert!(!text.fallback(), "{locale:?}");
    }
    let old = Timestamp(NOW.seconds() - 400 * DAY);
    assert_eq!(relative_at(old, NOW, Locale::German).as_str(), "am 4. Okt. 2025");
    let soon = Timestamp(NOW.seconds() + 2 * DAY);
    assert_eq!(until_at(soon, NOW, Locale::Russian).as_str(), "через 2 дня");
}

#[test]
fn dates_and_iso_attributes() {
    assert_eq!(date(NOW, Locale::Japanese).as_str(), "2026年11月8日");
    assert_eq!(date(NOW, Locale::PortugueseBrazil).as_str(), "8 de nov. de 2026");
    // The machine value never changes with the language.
    assert_eq!(When(NOW).iso(), "2026-11-08T12:00:00Z");
}

#[test]
fn counts_sizes_and_durations() {
    assert_eq!(count(Locale::English, 1_234_567), "1,234,567");
    assert_eq!(count(Locale::German, -1_234), "-1.234");
    assert_eq!(count(Locale::French, 1_234), "1\u{202f}234");
    assert_eq!(count(Locale::English, 999), "999");
    assert_eq!(bytes(Locale::English, 0), "0 B");
    assert_eq!(bytes(Locale::English, 1023), "1,023 B");
    assert_eq!(bytes(Locale::English, 1536), "1.5 KiB");
    assert_eq!(bytes(Locale::English, 5 * 1024 * 1024 + 300 * 1024), "5.2 MiB");
    assert_eq!(bytes(Locale::German, 1536), "1,5 KiB");
    assert_eq!(bytes(Locale::Russian, 3 * 1024 * 1024 * 1024), "3,0 GiB");
    assert_eq!(uptime(Locale::English, Duration::from_secs(61)).as_str(), "1 minute 1 second");
    assert_eq!(uptime(Locale::English, Duration::from_secs(3 * 3600 + 5 * 60 + 7)).as_str(), "3 hours 5 minutes");
    assert_eq!(uptime(Locale::Korean, Duration::from_secs(2 * 86_400 + 30)).as_str(), "2일");
    assert_eq!(elapsed(Locale::English, 63).as_str(), "1m 3s");
    assert_eq!(elapsed(Locale::German, 3_720).as_str(), "1 Std. 2 Min.");
    assert_eq!(elapsed(Locale::English, -5).as_str(), "0s");
}
