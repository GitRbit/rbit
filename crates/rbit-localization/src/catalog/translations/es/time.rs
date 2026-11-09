//! Spanish (`es`) entries for `catalog/time.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, month, write_duration, write_elapsed};
use crate::plural::Plural;

const MONTHS: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"];

/// "3 horas". `Many` (whole millions) shares the plural form.
fn amount(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    let one = text.plural(amount.count) == Plural::One;
    text.literal(match (amount.unit, one) {
        (TimeUnit::Second, true) => " segundo",
        (TimeUnit::Second, false) => " segundos",
        (TimeUnit::Minute, true) => " minuto",
        (TimeUnit::Minute, false) => " minutos",
        (TimeUnit::Hour, true) => " hora",
        (TimeUnit::Hour, false) => " horas",
        (TimeUnit::Day, true) => " día",
        (TimeUnit::Day, false) => " días",
        (TimeUnit::Week, true) => " semana",
        (TimeUnit::Week, false) => " semanas",
        (TimeUnit::Month, true) => " mes",
        (TimeUnit::Month, false) => " meses",
        (TimeUnit::Year, true) => " año",
        (TimeUnit::Year, false) => " años",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.day.into());
    text.literal(" ");
    text.literal(month(&MONTHS, date));
    text.literal(" ");
    text.digits(date.year.into());
}

entries! { Es:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("justo ahora"),
        TimeMessage::Ago(span) => {
            text.literal("hace ");
            amount(text, span);
        }
        TimeMessage::In(span) => {
            text.literal("dentro de ");
            amount(text, span);
        }
        TimeMessage::WithinAMinute => text.literal("en menos de un minuto"),
        TimeMessage::OnDate(day) => {
            text.literal("el ");
            date(text, day);
        }
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, amount, " y "),
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "h",
                TimeUnit::Minute => "min",
                _ => "s",
            },
            " ",
            " ",
        ),
    }
}
