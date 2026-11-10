//! Korean (`ko`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { Ko:
    RunLayoutMessage => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("요약"),
        RunLayoutMessage::Navigation => text.literal("실행 탐색"),
        RunLayoutMessage::Usage => text.literal("사용 시간"),
        RunLayoutMessage::WorkflowFile => text.literal("워크플로 파일"),
        RunLayoutMessage::JobCount { count } => {
            text.literal("작업 ");
            text.number(count);
            text.literal("개");
        }
    }
}
