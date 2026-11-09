//! Korean (`ko`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { Ko:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("푸시"),
        OverviewControlMessage::PullRequest => text.literal("풀 리퀘스트"),
        OverviewControlMessage::Manual => text.literal("수동"),
        OverviewControlMessage::Schedule => text.literal("예약"),
        OverviewControlMessage::WorkflowRun => text.literal("다른 워크플로 이후"),
        OverviewControlMessage::Dispatch => text.literal("디스패치"),
        OverviewControlMessage::Success => text.literal("성공"),
        OverviewControlMessage::Failure => text.literal("실패"),
        OverviewControlMessage::Cancelled => text.literal("취소됨"),
        OverviewControlMessage::Skipped => text.literal("건너뜀"),
        OverviewControlMessage::Queued => text.literal("대기열에 있음"),
        OverviewControlMessage::Running => text.literal("진행 중"),
        OverviewControlMessage::Waiting => text.literal("대기 중"),
        OverviewControlMessage::Approval => text.literal("승인 대기 중"),
        OverviewControlMessage::RunWorkflow => text.literal("워크플로 실행"),
        OverviewControlMessage::WorkflowFrom => text.literal("워크플로를 가져올 위치"),
        OverviewControlMessage::Change => text.literal("변경"),
    }
}
