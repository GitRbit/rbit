//! Korean (`ko`) entries for `catalog/job.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::job::ActionsJobMessage;

entries! { Ko:
    ActionsJobMessage => MessageWriter |text| {
        ActionsJobMessage::Details => text.literal("작업 세부 정보"),
        ActionsJobMessage::Steps => text.literal("단계 및 로그"),
        ActionsJobMessage::RunsOn => text.literal("실행 환경"),
        ActionsJobMessage::Duration => text.literal("소요 시간"),
        ActionsJobMessage::RawLog => text.literal("원본 로그"),
        ActionsJobMessage::Rerun => text.literal("작업 다시 실행"),
        ActionsJobMessage::NotStarted => text.literal("아직 시작되지 않음"),
        ActionsJobMessage::NotStartedDescription => text.literal("러너가 이 작업을 가져가면 단계와 로그가 여기에 표시됩니다."),
        ActionsJobMessage::RetainedWindow => text.literal("단계 미리 보기는 일부만 표시됩니다. 더 많은 줄을 보려면 단계를 열거나 원본 로그를 다운로드하세요."),
        ActionsJobMessage::Expired => text.literal("로그 보존 정책에 따라 이 로그가 삭제되었습니다."),
        ActionsJobMessage::NoOutput => text.literal("출력이 없습니다."),
        ActionsJobMessage::EarlierLines => text.literal("이전 줄은 생략되었습니다."),
        ActionsJobMessage::ShowMore => text.literal("이 단계 더 보기"),
        ActionsJobMessage::Or => text.literal(", 또는"),
        ActionsJobMessage::DownloadAll => text.literal("전체 내용은 원본 로그를 다운로드하세요."),
        ActionsJobMessage::Truncated => text.literal("작업 출력이 인스턴스의 보관 한도를 넘어 이후 줄은 버려졌습니다."),
        ActionsJobMessage::StepNotStarted => text.literal("이 단계는 아직 시작되지 않았습니다."),
        ActionsJobMessage::LogTools => text.literal("로그 도구"),
        ActionsJobMessage::SearchLogs => text.literal("로그 검색"),
        ActionsJobMessage::PreviousMatch => text.literal("이전 일치 항목"),
        ActionsJobMessage::NextMatch => text.literal("다음 일치 항목"),
        ActionsJobMessage::ShowTimestamps => text.literal("타임스탬프 표시"),
        ActionsJobMessage::ExpandAll => text.literal("모두 펼치기"),
        ActionsJobMessage::CollapseAll => text.literal("모두 접기"),
        ActionsJobMessage::DownloadLog => text.literal("로그 다운로드"),
        ActionsJobMessage::LinkCopied => text.literal("줄 링크를 복사했습니다"),
        ActionsJobMessage::NoMatches => text.literal("일치 항목 없음"),
        ActionsJobMessage::SetUpJob => text.literal("작업 설정"),
        ActionsJobMessage::CompleteJob => text.literal("작업 완료"),
        ActionsJobMessage::Success => text.literal("성공"),
        ActionsJobMessage::Failure => text.literal("실패"),
        ActionsJobMessage::Cancelled => text.literal("취소됨"),
        ActionsJobMessage::Skipped => text.literal("건너뜀"),
        ActionsJobMessage::Queued => text.literal("대기열에 있음"),
        ActionsJobMessage::Running => text.literal("진행 중"),
        ActionsJobMessage::Waiting => text.literal("대기 중"),
        ActionsJobMessage::Approval => text.literal("승인 대기 중"),
        ActionsJobMessage::WorkflowError => text.literal("워크플로 오류"),
        ActionsJobMessage::StepCount { count } => {
            text.literal("단계 ");
            text.number(count);
            text.literal("개");
        }
    }
}
