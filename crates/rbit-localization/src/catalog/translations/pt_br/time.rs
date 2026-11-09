//! Brazilian Portuguese (`pt-BR`) entries for `catalog/time.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, month, write_duration, write_elapsed};
use crate::plural::Plural;

const MONTHS: [&str; 12] = ["jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.", "dez."];

/// "3 horas". `One` covers 0 and 1; `Many` (whole millions) shares the plural form.
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
        (TimeUnit::Day, true) => " dia",
        (TimeUnit::Day, false) => " dias",
        (TimeUnit::Week, true) => " semana",
        (TimeUnit::Week, false) => " semanas",
        (TimeUnit::Month, true) => " mês",
        (TimeUnit::Month, false) => " meses",
        (TimeUnit::Year, true) => " ano",
        (TimeUnit::Year, false) => " anos",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.day.into());
    text.literal(" de ");
    text.literal(month(&MONTHS, date));
    text.literal(" de ");
    text.digits(date.year.into());
}

entries! { PtBr:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("agora mesmo"),
        TimeMessage::Ago(span) => {
            text.literal("há ");
            amount(text, span);
        }
        TimeMessage::In(span) => {
            text.literal("em ");
            amount(text, span);
        }
        TimeMessage::WithinAMinute => text.literal("em menos de um minuto"),
        TimeMessage::OnDate(day) => {
            text.literal("em ");
            date(text, day);
        }
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, amount, " e "),
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
