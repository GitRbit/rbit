//! Korean (`ko`) entries for `catalog/collab/labels.rs`.

use crate::catalog::collab::labels::LabelMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    LabelMessage<'a> => MessageWriter |text| {
        LabelMessage::NewLabel => text.literal("새 레이블"),
        LabelMessage::Name => text.literal("이름"),
        LabelMessage::Colour => text.literal("색상"),
        LabelMessage::CreateLabel => text.literal("레이블 만들기"),
        LabelMessage::Count { count, .. } => {
            text.literal("레이블 ");
            text.number(count);
            text.literal("개");
        }
        LabelMessage::EmptyTitle => text.literal("아직 레이블이 없습니다"),
        LabelMessage::EmptyText => text.literal("레이블로 이슈와 풀 리퀘스트를 분류합니다: bug, enhancement, good first issue…"),
        LabelMessage::CreateDefaults => text.literal("기본 레이블 만들기"),
        LabelMessage::OpenIssues { count, .. } => {
            text.literal("열린 이슈 ");
            text.number(count);
            text.literal("개");
        }
        LabelMessage::DeleteConfirm { name, .. } => {
            text.literal("레이블 “");
            text.argument(name);
            text.literal("”을(를) 삭제할까요? 모든 이슈에서 제거됩니다.");
        }
    }
}
