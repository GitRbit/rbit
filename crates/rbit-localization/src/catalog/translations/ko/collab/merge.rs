//! Korean (`ko`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("병합"),
        MergeMessage::ConflictingFiles => text.literal("충돌하는 파일"),
        MergeMessage::WorkInProgress => text.literal("이 풀 리퀘스트는 아직 작업 중입니다."),
        MergeMessage::ReadyForReview => text.literal("리뷰 준비 완료"),
        MergeMessage::MergeMethod => text.literal("병합 방법"),
        MergeMessage::CreateMergeCommit => text.literal("병합 커밋 만들기"),
        MergeMessage::SquashAndMerge => text.literal("스쿼시 후 병합"),
        MergeMessage::RebaseAndMerge => text.literal("리베이스 후 병합"),
        MergeMessage::EditMessage => text.literal("커밋 메시지 편집"),
        MergeMessage::Message => text.literal("메시지"),
        MergeMessage::TitlePlaceholder { title, squash, .. } => {
            text.argument(title);
            text.literal(" — 스쿼시할 때는 ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("병합 후 헤드 브랜치 삭제"),
        MergeMessage::Bypass => text.literal("브랜치 보호 요구 사항을 충족하지 않고 병합(감사 기록에 남음)"),
        MergeMessage::MergePull => text.literal("풀 리퀘스트 병합"),
        MergeMessage::WriteAccessOnly => text.literal("이 저장소에 쓰기 권한이 있는 사용자만 병합할 수 있습니다."),
        MergeMessage::PullOpen => text.literal("풀 리퀘스트가 열려 있음"),
        MergeMessage::PullClosed => text.literal("풀 리퀘스트가 닫힘"),
        MergeMessage::StillDraft => text.literal("이 풀 리퀘스트는 아직 초안임"),
        MergeMessage::HeadExists => text.literal("헤드 브랜치가 있음"),
        MergeMessage::HeadDeleted => text.literal("헤드 브랜치가 삭제됨"),
        MergeMessage::NoConflicts => text.literal("베이스 브랜치와 충돌 없음"),
        MergeMessage::Conflicts => text.literal("이 브랜치에 해결해야 할 충돌이 있음"),
        MergeMessage::CheckingConflicts => text.literal("충돌 확인 중…"),
        MergeMessage::MergeabilityError => text.literal("병합 가능 여부를 확인할 수 없음"),
        MergeMessage::Approvals { have, need, .. } => {
            text.literal("필수 승인 ");
            text.number(need);
            text.literal("개 중 ");
            text.number(have);
            text.literal("개");
        }
        MergeMessage::NoChangesRequested => text.literal("변경 요청 없음"),
        MergeMessage::ChangesRequestedBy { count, .. } => {
            text.literal("리뷰어 ");
            text.number(count);
            text.literal("명이 변경을 요청함");
        }
        MergeMessage::MayPush => text.literal("베이스 브랜치에 푸시할 수 있음"),
        MergeMessage::MayNotPush => text.literal("베이스 브랜치에 푸시할 수 없음"),
        MergeMessage::NoChecks => text.literal("상태 검사 없음"),
        MergeMessage::ChecksPending => text.literal("일부 검사가 아직 완료되지 않음"),
        MergeMessage::ChecksPassed => text.literal("모든 검사 통과"),
        MergeMessage::ChecksFailed => text.literal("일부 검사가 성공하지 못함"),
        MergeMessage::RequiredPassed { count, .. } => {
            text.literal("필수 검사 ");
            text.number(count);
            text.literal("개 통과");
        }
        MergeMessage::RequiredFailed { failing, required, .. } => {
            text.literal("필수 검사 ");
            text.number(required);
            text.literal("개 중 ");
            text.number(failing);
            text.literal("개 실패");
        }
        MergeMessage::RequiredWaiting { waiting, required, .. } => {
            text.literal("필수 검사 ");
            text.number(required);
            text.literal("개 중 ");
            text.number(waiting);
            text.literal("개 대기 중");
        }
    }
}
