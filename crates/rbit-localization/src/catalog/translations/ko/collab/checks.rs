//! Korean (`ko`) entries for `catalog/collab/checks.rs`.

use crate::catalog::collab::checks::ChecksMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ChecksMessage => MessageWriter |text| {
        ChecksMessage::Summary { successful, failing, pending, expected } => {
            let parts = [(successful, "개 성공"), (failing, "개 실패"), (pending, "개 대기 중")];
            if parts.iter().all(|(count, _)| *count == 0) {
                text.literal("검사 없음");
            } else {
                text.literal("검사 ");
                let mut first = true;
                for (count, word) in parts {
                    if count > 0 {
                        if !first {
                            text.literal(", ");
                        }
                        first = false;
                        text.number(count);
                        text.literal(word);
                    }
                }
            }
            if expected > 0 {
                text.literal(", 보고 예정 ");
                text.number(expected);
                text.literal("개");
            }
        }
        ChecksMessage::Success => text.literal("성공"),
        ChecksMessage::Failure => text.literal("실패"),
        ChecksMessage::Error => text.literal("오류"),
        ChecksMessage::Pending => text.literal("대기 중"),
        ChecksMessage::Expected => text.literal("보고 예정"),
        ChecksMessage::ExpectedDescription => text.literal("이 상태가 보고되기를 기다리는 중"),
        ChecksMessage::Required => text.literal("필수"),
        ChecksMessage::Details => text.literal("세부 정보"),
    }
}
