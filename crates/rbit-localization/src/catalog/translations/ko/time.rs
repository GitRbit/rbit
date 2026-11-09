//! Korean (`ko`) entries for `catalog/time.rs`. Korean has no plural forms.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, write_duration, write_elapsed};

/// "3시간", with the unit attached to the number.
fn amount(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    text.literal(match amount.unit {
        TimeUnit::Second => "초",
        TimeUnit::Minute => "분",
        TimeUnit::Hour => "시간",
        TimeUnit::Day => "일",
        TimeUnit::Week => "주",
        TimeUnit::Month => "개월",
        TimeUnit::Year => "년",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.year.into());
    text.literal("년 ");
    text.digits(date.month.into());
    text.literal("월 ");
    text.digits(date.day.into());
    text.literal("일");
}

entries! { Ko:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("방금"),
        TimeMessage::Ago(span) => {
            amount(text, span);
            text.literal(" 전");
        }
        TimeMessage::In(span) => {
            amount(text, span);
            text.literal(" 후");
        }
        TimeMessage::WithinAMinute => text.literal("1분 이내"),
        TimeMessage::OnDate(day) => date(text, day),
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, amount, " "),
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "시간",
                TimeUnit::Minute => "분",
                _ => "초",
            },
            "",
            " ",
        ),
    }
}
