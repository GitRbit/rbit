//! Russian (`ru`) entries for `catalog/time.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, month, write_duration, write_elapsed};
use crate::plural::Plural;

/// Genitive abbreviations, as a date reads them ("8 нояб.").
const MONTHS: [&str; 12] = ["янв.", "февр.", "мар.", "апр.", "мая", "июн.", "июл.", "авг.", "сент.", "окт.", "нояб.", "дек."];

/// The forms for one, few and many of each unit, accusative after "назад" and "через"
/// ("5 минут назад", "через 1 минуту").
const fn accusative(unit: TimeUnit) -> [&'static str; 3] {
    match unit {
        TimeUnit::Second => [" секунду", " секунды", " секунд"],
        TimeUnit::Minute => [" минуту", " минуты", " минут"],
        TimeUnit::Hour => [" час", " часа", " часов"],
        TimeUnit::Day => [" день", " дня", " дней"],
        TimeUnit::Week => [" неделю", " недели", " недель"],
        TimeUnit::Month => [" месяц", " месяца", " месяцев"],
        TimeUnit::Year => [" год", " года", " лет"],
    }
}

/// The nominative of a duration ("1 минута", "3 часа 5 минут").
const fn nominative(unit: TimeUnit) -> [&'static str; 3] {
    match unit {
        TimeUnit::Second => [" секунда", " секунды", " секунд"],
        TimeUnit::Minute => [" минута", " минуты", " минут"],
        TimeUnit::Week => [" неделя", " недели", " недель"],
        other => accusative(other),
    }
}

fn write(text: &mut MessageWriter, amount: Amount, forms: [&'static str; 3]) {
    text.number(amount.count);
    text.literal(match text.plural(amount.count) {
        Plural::One => forms[0],
        Plural::Few => forms[1],
        _ => forms[2],
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.day.into());
    text.literal(" ");
    text.literal(month(&MONTHS, date));
    text.literal(" ");
    text.digits(date.year.into());
    text.literal(" г.");
}

entries! { Ru:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("только что"),
        TimeMessage::Ago(amount) => {
            write(text, amount, accusative(amount.unit));
            text.literal(" назад");
        }
        TimeMessage::In(amount) => {
            text.literal("через ");
            write(text, amount, accusative(amount.unit));
        }
        TimeMessage::WithinAMinute => text.literal("в течение минуты"),
        TimeMessage::OnDate(day) => date(text, day),
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => {
            write_duration(text, major, minor, |text, amount| write(text, amount, nominative(amount.unit)), " ");
        }
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "ч",
                TimeUnit::Minute => "мин",
                _ => "с",
            },
            " ",
            " ",
        ),
    }
}
