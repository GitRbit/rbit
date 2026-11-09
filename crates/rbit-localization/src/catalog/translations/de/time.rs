//! German (`de`) entries for `catalog/time.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, month, write_duration, write_elapsed};
use crate::plural::Plural;

const MONTHS: [&str; 12] = ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."];

/// "3 Stunden" after "vor" and "in", which take the dative ("vor 2 Tagen").
fn dative(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    let one = text.plural(amount.count) == Plural::One;
    text.literal(match (amount.unit, one) {
        (TimeUnit::Second, true) => " Sekunde",
        (TimeUnit::Second, false) => " Sekunden",
        (TimeUnit::Minute, true) => " Minute",
        (TimeUnit::Minute, false) => " Minuten",
        (TimeUnit::Hour, true) => " Stunde",
        (TimeUnit::Hour, false) => " Stunden",
        (TimeUnit::Day, true) => " Tag",
        (TimeUnit::Day, false) => " Tagen",
        (TimeUnit::Week, true) => " Woche",
        (TimeUnit::Week, false) => " Wochen",
        (TimeUnit::Month, true) => " Monat",
        (TimeUnit::Month, false) => " Monaten",
        (TimeUnit::Year, true) => " Jahr",
        (TimeUnit::Year, false) => " Jahren",
    });
}

/// "3 Tage", the nominative of a duration; it differs from the dative only in the plural
/// of days, months and years.
fn nominative(text: &mut MessageWriter, amount: Amount) {
    if text.plural(amount.count) == Plural::One || !matches!(amount.unit, TimeUnit::Day | TimeUnit::Month | TimeUnit::Year) {
        return dative(text, amount);
    }
    text.number(amount.count);
    text.literal(match amount.unit {
        TimeUnit::Day => " Tage",
        TimeUnit::Month => " Monate",
        _ => " Jahre",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.day.into());
    text.literal(". ");
    text.literal(month(&MONTHS, date));
    text.literal(" ");
    text.digits(date.year.into());
}

entries! { De:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("gerade eben"),
        TimeMessage::Ago(amount) => {
            text.literal("vor ");
            dative(text, amount);
        }
        TimeMessage::In(amount) => {
            text.literal("in ");
            dative(text, amount);
        }
        TimeMessage::WithinAMinute => text.literal("in weniger als einer Minute"),
        TimeMessage::OnDate(day) => {
            text.literal("am ");
            date(text, day);
        }
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, nominative, " "),
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "Std.",
                TimeUnit::Minute => "Min.",
                _ => "Sek.",
            },
            " ",
            " ",
        ),
    }
}
