//! French (`fr`) entries for `catalog/time.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, month, write_duration, write_elapsed};
use crate::plural::Plural;

const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/// "3 heures". `One` covers 0 and 1; `Many` (whole millions) shares the plural form.
fn amount(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    let one = text.plural(amount.count) == Plural::One;
    text.literal(match (amount.unit, one) {
        (TimeUnit::Second, true) => " seconde",
        (TimeUnit::Second, false) => " secondes",
        (TimeUnit::Minute, true) => " minute",
        (TimeUnit::Minute, false) => " minutes",
        (TimeUnit::Hour, true) => " heure",
        (TimeUnit::Hour, false) => " heures",
        (TimeUnit::Day, true) => " jour",
        (TimeUnit::Day, false) => " jours",
        (TimeUnit::Week, true) => " semaine",
        (TimeUnit::Week, false) => " semaines",
        (TimeUnit::Month, _) => " mois",
        (TimeUnit::Year, true) => " an",
        (TimeUnit::Year, false) => " ans",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.day.into());
    text.literal(" ");
    text.literal(month(&MONTHS, date));
    text.literal(" ");
    text.digits(date.year.into());
}

entries! { Fr:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("à l’instant"),
        TimeMessage::Ago(span) => {
            text.literal("il y a ");
            amount(text, span);
        }
        TimeMessage::In(span) => {
            text.literal("dans ");
            amount(text, span);
        }
        TimeMessage::WithinAMinute => text.literal("dans moins d’une minute"),
        TimeMessage::OnDate(day) => {
            text.literal("le ");
            date(text, day);
        }
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, amount, " et "),
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "h",
                TimeUnit::Minute => "min",
                _ => "s",
            },
            "\u{a0}",
            " ",
        ),
    }
}
