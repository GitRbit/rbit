//! Relative times, dates, durations and numbers in every language, at the plural
//! boundaries each language distinguishes (0, 1, 2, 5, 21, 101).
use rbit_localization::time::{Amount, CivilDate, TimeMessage, TimeUnit};
use rbit_localization::{Catalog, Locale, number};

const BOUNDARIES: [u64; 6] = [0, 1, 2, 5, 21, 101];
const NOV_8: CivilDate = CivilDate { year: 2026, month: 11, day: 8 };

fn text(locale: Locale, message: TimeMessage) -> String {
    let localized = Catalog::new(locale).localized(message);
    assert!(!localized.fallback(), "{message:?} is translated in {locale:?}");
    localized.to_string()
}

fn ago(locale: Locale, unit: TimeUnit) -> Vec<String> {
    BOUNDARIES.iter().map(|&count| text(locale, TimeMessage::Ago(Amount::new(count, unit)))).collect()
}

fn later(locale: Locale, unit: TimeUnit) -> Vec<String> {
    BOUNDARIES.iter().map(|&count| text(locale, TimeMessage::In(Amount::new(count, unit)))).collect()
}

#[test]
fn every_language_has_every_phrase() {
    let date = NOV_8;
    for locale in Locale::LANGUAGES {
        for unit in TimeUnit::ALL {
            for count in BOUNDARIES {
                let amount = Amount::new(count, unit);
                for message in [TimeMessage::Ago(amount), TimeMessage::In(amount), TimeMessage::Duration { major: amount, minor: None }] {
                    assert!(text(locale, message).contains(&count.to_string()), "{message:?} in {locale:?}");
                }
            }
        }
        for message in [TimeMessage::JustNow, TimeMessage::WithinAMinute, TimeMessage::OnDate(date), TimeMessage::Elapsed { seconds: 63 }] {
            assert_ne!(text(locale, message), "", "{message:?} in {locale:?}");
        }
    }
}

#[test]
fn english_keeps_its_wording() {
    let en = Locale::English;
    assert_eq!(
        ago(en, TimeUnit::Minute),
        ["0 minutes ago", "1 minute ago", "2 minutes ago", "5 minutes ago", "21 minutes ago", "101 minutes ago"]
    );
    assert_eq!(later(en, TimeUnit::Day)[1], "in 1 day");
    assert_eq!(text(en, TimeMessage::JustNow), "just now");
    assert_eq!(text(en, TimeMessage::WithinAMinute), "in under a minute");
    assert_eq!(text(en, TimeMessage::OnDate(NOV_8)), "on Nov 8, 2026");
}

#[test]
fn german_uses_the_dative_after_vor_and_in() {
    let de = Locale::German;
    assert_eq!(
        ago(de, TimeUnit::Minute),
        ["vor 0 Minuten", "vor 1 Minute", "vor 2 Minuten", "vor 5 Minuten", "vor 21 Minuten", "vor 101 Minuten"]
    );
    assert_eq!(later(de, TimeUnit::Day), ["in 0 Tagen", "in 1 Tag", "in 2 Tagen", "in 5 Tagen", "in 21 Tagen", "in 101 Tagen"]);
    assert_eq!(text(de, TimeMessage::duration(2 * 86_400 + 3 * 3_600)), "2 Tage 3 Stunden");
}

#[test]
fn spanish_and_portuguese_choose_one_differently_for_zero() {
    let es = Locale::Spanish;
    assert_eq!(ago(es, TimeUnit::Hour), ["hace 0 horas", "hace 1 hora", "hace 2 horas", "hace 5 horas", "hace 21 horas", "hace 101 horas"]);
    assert_eq!(later(es, TimeUnit::Month)[1], "dentro de 1 mes");
    let pt = Locale::PortugueseBrazil;
    assert_eq!(ago(pt, TimeUnit::Day), ["há 0 dia", "há 1 dia", "há 2 dias", "há 5 dias", "há 21 dias", "há 101 dias"]);
    assert_eq!(later(pt, TimeUnit::Month), ["em 0 mês", "em 1 mês", "em 2 meses", "em 5 meses", "em 21 meses", "em 101 meses"]);
}

#[test]
fn french_treats_zero_and_one_as_one() {
    let fr = Locale::French;
    assert_eq!(ago(fr, TimeUnit::Year), ["il y a 0 an", "il y a 1 an", "il y a 2 ans", "il y a 5 ans", "il y a 21 ans", "il y a 101 ans"]);
    assert_eq!(later(fr, TimeUnit::Month), ["dans 0 mois", "dans 1 mois", "dans 2 mois", "dans 5 mois", "dans 21 mois", "dans 101 mois"]);
    assert_eq!(text(fr, TimeMessage::JustNow), "à l’instant");
}

#[test]
fn russian_has_one_few_and_many() {
    let ru = Locale::Russian;
    assert_eq!(
        ago(ru, TimeUnit::Minute),
        ["0 минут назад", "1 минуту назад", "2 минуты назад", "5 минут назад", "21 минуту назад", "101 минуту назад"]
    );
    assert_eq!(ago(ru, TimeUnit::Day), ["0 дней назад", "1 день назад", "2 дня назад", "5 дней назад", "21 день назад", "101 день назад"]);
    assert_eq!(later(ru, TimeUnit::Year), ["через 0 лет", "через 1 год", "через 2 года", "через 5 лет", "через 21 год", "через 101 год"]);
    assert_eq!(text(ru, TimeMessage::Ago(Amount::new(11, TimeUnit::Hour))), "11 часов назад");
    assert_eq!(text(ru, TimeMessage::Ago(Amount::new(22, TimeUnit::Hour))), "22 часа назад");
    assert_eq!(text(ru, TimeMessage::duration(61)), "1 минута 1 секунда");
}

#[test]
fn chinese_japanese_and_korean_have_one_form() {
    assert_eq!(ago(Locale::Japanese, TimeUnit::Hour), ["0 時間前", "1 時間前", "2 時間前", "5 時間前", "21 時間前", "101 時間前"]);
    assert_eq!(later(Locale::Japanese, TimeUnit::Day)[2], "2 日後");
    assert_eq!(ago(Locale::ChineseSimplified, TimeUnit::Minute), ["0分钟前", "1分钟前", "2分钟前", "5分钟前", "21分钟前", "101分钟前"]);
    assert_eq!(later(Locale::ChineseSimplified, TimeUnit::Month)[5], "101个月后");
    assert_eq!(ago(Locale::Korean, TimeUnit::Day), ["0일 전", "1일 전", "2일 전", "5일 전", "21일 전", "101일 전"]);
    assert_eq!(later(Locale::Korean, TimeUnit::Week)[1], "1주 후");
}

#[test]
fn dates_follow_each_language() {
    let expected = [
        (Locale::English, "Nov 8, 2026"),
        (Locale::German, "8. Nov. 2026"),
        (Locale::Spanish, "8 nov 2026"),
        (Locale::French, "8 nov. 2026"),
        (Locale::PortugueseBrazil, "8 de nov. de 2026"),
        (Locale::Russian, "8 нояб. 2026 г."),
        (Locale::Japanese, "2026年11月8日"),
        (Locale::ChineseSimplified, "2026年11月8日"),
        (Locale::Korean, "2026년 11월 8일"),
    ];
    for (locale, date) in expected {
        assert_eq!(text(locale, TimeMessage::Date(NOV_8)), date, "{locale:?}");
    }
    let may = CivilDate { year: 2027, month: 5, day: 21 };
    assert_eq!(text(Locale::Russian, TimeMessage::Date(may)), "21 мая 2027 г.");
    assert_eq!(text(Locale::German, TimeMessage::OnDate(may)), "am 21. Mai 2027");
    // A year is a label, never grouped like a count.
    assert_eq!(text(Locale::German, TimeMessage::Date(CivilDate { year: 12_026, month: 1, day: 1 })), "1. Jan. 12026");
}

#[test]
fn durations_and_elapsed_times() {
    let uptime = TimeMessage::duration(3 * 3_600 + 5 * 60 + 7);
    let expected = [
        (Locale::English, "3 hours 5 minutes", "1h 2m"),
        (Locale::German, "3 Stunden 5 Minuten", "1 Std. 2 Min."),
        (Locale::Spanish, "3 horas y 5 minutos", "1 h 2 min"),
        (Locale::French, "3 heures et 5 minutes", "1\u{a0}h 2\u{a0}min"),
        (Locale::PortugueseBrazil, "3 horas e 5 minutos", "1 h 2 min"),
        (Locale::Russian, "3 часа 5 минут", "1 ч 2 мин"),
        (Locale::Japanese, "3 時間 5 分", "1時間2分"),
        (Locale::ChineseSimplified, "3小时5分钟", "1小时2分钟"),
        (Locale::Korean, "3시간 5분", "1시간 2분"),
    ];
    for (locale, duration, elapsed) in expected {
        assert_eq!(text(locale, uptime), duration, "{locale:?}");
        assert_eq!(text(locale, TimeMessage::Elapsed { seconds: 3_720 }), elapsed, "{locale:?}");
    }
    assert_eq!(text(Locale::English, TimeMessage::Elapsed { seconds: 63 }), "1m 3s");
    assert_eq!(text(Locale::English, TimeMessage::Elapsed { seconds: 12 }), "12s");
    assert_eq!(text(Locale::English, TimeMessage::duration(0)), "0 seconds");
    assert_eq!(text(Locale::English, TimeMessage::duration(2 * 86_400 + 30)), "2 days");
}

#[test]
fn spans_pick_the_same_unit_in_every_language() {
    assert_eq!(TimeMessage::past(59), Some(TimeMessage::JustNow));
    assert_eq!(TimeMessage::past(3 * 3_600 + 59), Some(TimeMessage::Ago(Amount::new(3, TimeUnit::Hour))));
    assert_eq!(TimeMessage::past(400 * 86_400), None);
    assert_eq!(TimeMessage::future(30), Some(TimeMessage::WithinAMinute));
    assert_eq!(TimeMessage::future(90 * 86_400 - 3_600), Some(TimeMessage::In(Amount::new(3, TimeUnit::Month))));
    assert_eq!(TimeMessage::future(400 * 86_400), None);
}

#[test]
fn numbers_group_by_language() {
    let expected = [
        (Locale::English, "1,234", "1,234,567", "1.5"),
        (Locale::German, "1.234", "1.234.567", "1,5"),
        (Locale::Spanish, "1234", "1.234.567", "1,5"),
        (Locale::French, "1\u{202f}234", "1\u{202f}234\u{202f}567", "1,5"),
        (Locale::PortugueseBrazil, "1.234", "1.234.567", "1,5"),
        (Locale::Russian, "1\u{a0}234", "1\u{a0}234\u{a0}567", "1,5"),
        (Locale::Japanese, "1,234", "1,234,567", "1.5"),
        (Locale::ChineseSimplified, "1,234", "1,234,567", "1.5"),
        (Locale::Korean, "1,234", "1,234,567", "1.5"),
    ];
    for (locale, thousands, millions, decimal) in expected {
        assert_eq!(number::integer(locale, 1_234), thousands, "{locale:?}");
        assert_eq!(number::integer(locale, 1_234_567), millions, "{locale:?}");
        assert_eq!(number::tenths(locale, 15), decimal, "{locale:?}");
        assert_eq!(number::integer(locale, 999), "999");
        assert_eq!(number::integer(locale, 0), "0");
    }
    assert_eq!(number::integer(Locale::Spanish, 12_345), "12.345");
    assert_eq!(number::signed(Locale::German, -1_234), "-1.234");
    assert_eq!(number::integer(Locale::English, u64::MAX), "18,446,744,073,709,551,615");
    // Counts inside sentences group the same way.
    assert_eq!(text(Locale::German, TimeMessage::Ago(Amount::new(1_500, TimeUnit::Day))), "vor 1.500 Tagen");
}

#[test]
fn diagnostics_derive_from_english() {
    let expanded = Catalog::new(Locale::PseudoExpanded).localized(TimeMessage::JustNow);
    assert!(!expanded.fallback() && expanded.as_str().starts_with('['));
    let rtl = Catalog::new(Locale::PseudoRtl).localized(TimeMessage::Ago(Amount::new(1_234, TimeUnit::Day)));
    assert_eq!(rtl.as_str(), "[1,234 days ago]");
}
