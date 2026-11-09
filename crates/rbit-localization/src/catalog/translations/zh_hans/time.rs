//! Simplified Chinese (`zh-Hans`) entries for `catalog/time.rs`. Chinese has no plural forms.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::time::{Amount, CivilDate, TimeMessage, TimeUnit, write_duration, write_elapsed};

/// "3小时", with no space, as CLDR writes it.
fn amount(text: &mut MessageWriter, amount: Amount) {
    text.number(amount.count);
    text.literal(match amount.unit {
        TimeUnit::Second => "秒钟",
        TimeUnit::Minute => "分钟",
        TimeUnit::Hour => "小时",
        TimeUnit::Day => "天",
        TimeUnit::Week => "周",
        TimeUnit::Month => "个月",
        TimeUnit::Year => "年",
    });
}

fn date(text: &mut MessageWriter, date: CivilDate) {
    text.digits(date.year.into());
    text.literal("年");
    text.digits(date.month.into());
    text.literal("月");
    text.digits(date.day.into());
    text.literal("日");
}

entries! { ZhHans:
    TimeMessage => MessageWriter |text| {
        TimeMessage::JustNow => text.literal("刚刚"),
        TimeMessage::Ago(span) => {
            amount(text, span);
            text.literal("前");
        }
        TimeMessage::In(span) => {
            amount(text, span);
            text.literal("后");
        }
        TimeMessage::WithinAMinute => text.literal("1分钟内"),
        TimeMessage::OnDate(day) => date(text, day),
        TimeMessage::Date(day) => date(text, day),
        TimeMessage::Duration { major, minor } => write_duration(text, major, minor, amount, ""),
        TimeMessage::Elapsed { seconds } => write_elapsed(
            text,
            seconds,
            |unit| match unit {
                TimeUnit::Hour => "小时",
                TimeUnit::Minute => "分钟",
                _ => "秒",
            },
            "",
            "",
        ),
    }
}
