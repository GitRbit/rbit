//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;

/// "检查：3 项成功、1 项失败；2 项必需检查尚未报告".
fn summary(text: &mut MessageWriter, successful: u64, failing: u64, pending: u64, expected: u64) {
    let parts: Vec<(u64, &str)> =
        [(successful, " 项成功"), (failing, " 项失败"), (pending, " 项等待中")].into_iter().filter(|(count, _)| *count > 0).collect();
    if parts.is_empty() {
        text.literal("没有检查");
    } else {
        text.literal("检查：");
    }
    for (index, (count, word)) in parts.iter().enumerate() {
        if index > 0 {
            text.literal("、");
        }
        text.number(*count);
        text.literal(word);
    }
    if expected > 0 {
        text.literal("；");
        text.number(expected);
        text.literal(" 项必需检查尚未报告");
    }
}

entries! { ZhHans:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => summary(text, successful, failing, pending, expected),
        ChecksMessage::Success => text.literal("成功"),
        ChecksMessage::Failure => text.literal("失败"),
        ChecksMessage::Error => text.literal("错误"),
        ChecksMessage::Pending => text.literal("等待中"),
        ChecksMessage::Expected => text.literal("等待报告"),
        ChecksMessage::ExpectedDescription => text.literal("正在等待报告此状态"),
        ChecksMessage::Required => text.literal("必需"),
        ChecksMessage::Details => text.literal("详情"),
    }
}
