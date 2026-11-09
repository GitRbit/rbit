//! Japanese (`ja`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            let mut first = true;
            for (word, count) in [("成功 ", successful), ("失敗 ", failing), ("保留中 ", pending)] {
                if count > 0 {
                    if !first {
                        text.literal("、");
                    }
                    first = false;
                    text.literal(word);
                    text.number(count);
                    text.literal(" 件");
                }
            }
            if first {
                text.literal("チェックなし");
            }
            if expected > 0 {
                text.literal("、報告待ち ");
                text.number(expected);
                text.literal(" 件");
            }
        }
        ChecksMessage::Success => text.literal("成功"),
        ChecksMessage::Failure => text.literal("失敗"),
        ChecksMessage::Error => text.literal("エラー"),
        ChecksMessage::Pending => text.literal("保留中"),
        ChecksMessage::Expected => text.literal("報告待ち"),
        ChecksMessage::ExpectedDescription => text.literal("このステータスの報告を待っています"),
        ChecksMessage::Required => text.literal("必須"),
        ChecksMessage::Details => text.literal("詳細"),
    }
}
