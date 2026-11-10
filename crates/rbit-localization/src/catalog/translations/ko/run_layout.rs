//! Korean (`ko`) entries for `catalog/run_layout.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::run_layout::RunLayoutMessage;

entries! { Ko:
    RunLayoutMessage<'a> => MessageWriter |text| {
        RunLayoutMessage::Summary => text.literal("요약"),
        RunLayoutMessage::Navigation => text.literal("실행 탐색"),
        RunLayoutMessage::Usage => text.literal("사용 시간"),
        RunLayoutMessage::WorkflowFile => text.literal("워크플로 파일"),
        RunLayoutMessage::JobCount { count } => {
            text.literal("작업 ");
            text.number(count);
            text.literal("개");
        }
        RunLayoutMessage::Graph => text.literal("작업 그래프"),
        RunLayoutMessage::MoreJobs { count } => {
            text.literal("외 ");
            text.number(count);
            text.literal("개");
        }
        RunLayoutMessage::Annotations => text.literal("주석"),
        RunLayoutMessage::Errors { count } => {
            text.literal("오류 ");
            text.number(count);
            text.literal("개");
        }
        RunLayoutMessage::Warnings { count } => {
            text.literal("경고 ");
            text.number(count);
            text.literal("개");
        }
        RunLayoutMessage::Notices { count } => {
            text.literal("알림 ");
            text.number(count);
            text.literal("개");
        }
        RunLayoutMessage::ErrorLevel => text.literal("오류"),
        RunLayoutMessage::WarningLevel => text.literal("경고"),
        RunLayoutMessage::NoticeLevel => text.literal("알림"),
        RunLayoutMessage::StepFailed { step } => {
            text.literal("단계 ");
            text.argument(step);
            text.literal("이(가) 실패했습니다.");
        }
        RunLayoutMessage::MoreAnnotations { count } => {
            text.literal("작업 로그에 ");
            text.number(count);
            text.literal("개가 더 있습니다.");
        }
        RunLayoutMessage::TriggeredVia => text.literal("트리거"),
        RunLayoutMessage::Status => text.literal("상태"),
        RunLayoutMessage::TotalDuration => text.literal("총 소요 시간"),
        RunLayoutMessage::RerunBy { name } => {
            text.argument(name);
            text.literal(" 님이 다시 실행");
        }
        RunLayoutMessage::Inputs => text.literal("입력"),
        RunLayoutMessage::Waited { time } => {
            text.literal("대기 ");
            text.argument(time);
        }
        RunLayoutMessage::RanOn { runner } => {
            text.literal("러너 ");
            text.argument(runner);
        }
    }
}
