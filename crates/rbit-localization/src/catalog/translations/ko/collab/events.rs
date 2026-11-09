//! Korean (`ko`) entries for `catalog/collab/events.rs`.

use crate::catalog::collab::events::{CloseReason, TimelineEventMessage, TimelineNoteMessage};
use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    TimelineEventMessage => RichWriter |text| {
        TimelineEventMessage::Closed { reason } => {
            text.slot(0);
            text.literal(" 님이 ");
            text.literal(closed_as(reason));
        }
        TimelineEventMessage::ClosedIn { reason } => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("에서 ");
            text.literal(closed_as(reason));
        }
        TimelineEventMessage::Reopened => {
            text.slot(0);
            text.literal(" 님이 이 항목을 다시 열었습니다");
        }
        TimelineEventMessage::Renamed => {
            text.slot(0);
            text.literal(" 님이 제목을 ");
            text.slot(1);
            text.literal("에서 ");
            text.slot(2);
            text.literal("(으)로 변경했습니다");
        }
        TimelineEventMessage::Locked => {
            text.slot(0);
            text.literal(" 님이 대화를 잠그고 공동 작업자로 제한했습니다");
        }
        TimelineEventMessage::Unlocked => {
            text.slot(0);
            text.literal(" 님이 이 대화의 잠금을 해제했습니다");
        }
        TimelineEventMessage::Labeled => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 레이블을 추가했습니다");
        }
        TimelineEventMessage::Unlabeled => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 레이블을 제거했습니다");
        }
        TimelineEventMessage::Milestoned => {
            text.slot(0);
            text.literal(" 님이 이 항목을 ");
            text.slot(1);
            text.literal(" 마일스톤에 추가했습니다");
        }
        TimelineEventMessage::Demilestoned => {
            text.slot(0);
            text.literal(" 님이 이 항목을 ");
            text.slot(1);
            text.literal(" 마일스톤에서 제거했습니다");
        }
        TimelineEventMessage::SelfAssigned => {
            text.slot(0);
            text.literal(" 님이 자신을 담당자로 지정했습니다");
        }
        TimelineEventMessage::Assigned => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 님을 담당자로 지정했습니다");
        }
        TimelineEventMessage::SelfUnassigned => {
            text.slot(0);
            text.literal(" 님이 담당자 지정을 해제했습니다");
        }
        TimelineEventMessage::Unassigned => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 님을 담당자에서 해제했습니다");
        }
        TimelineEventMessage::CommitReferenced => {
            text.slot(0);
            text.literal(" 님이 커밋 ");
            text.slot(1);
            text.literal("에서 이 항목을 참조했습니다");
        }
        TimelineEventMessage::MentionedInIssue => {
            text.slot(0);
            text.literal(" 님이 이슈 ");
            text.slot(1);
            text.literal("에서 이 항목을 언급했습니다");
        }
        TimelineEventMessage::MentionedInPull => {
            text.slot(0);
            text.literal(" 님이 풀 리퀘스트 ");
            text.slot(1);
            text.literal("에서 이 항목을 언급했습니다");
        }
        TimelineEventMessage::Mentioned => {
            text.slot(0);
            text.literal(" 님이 이 항목을 언급했습니다");
        }
        TimelineEventMessage::ForcePushed => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(2);
            text.literal("에서 ");
            text.slot(3);
            text.literal("(으)로 강제 푸시했습니다");
        }
        TimelineEventMessage::ForcePushedBranch => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 브랜치를 ");
            text.slot(2);
            text.literal("에서 ");
            text.slot(3);
            text.literal("(으)로 강제 푸시했습니다");
        }
        TimelineEventMessage::Pushed { count, .. } => {
            text.slot(0);
            text.literal(" 님이 커밋 ");
            text.number(count);
            text.literal("개를 추가했습니다. 현재 ");
            text.slot(2);
        }
        TimelineEventMessage::PushedTo { count, .. } => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("에 커밋 ");
            text.number(count);
            text.literal("개를 추가했습니다. 현재 ");
            text.slot(2);
        }
        TimelineEventMessage::BaseChanged => {
            text.slot(0);
            text.literal(" 님이 베이스 브랜치를 ");
            text.slot(1);
            text.literal("에서 ");
            text.slot(2);
            text.literal("(으)로 변경했습니다");
        }
        TimelineEventMessage::Merged => {
            text.slot(0);
            text.literal(" 님이 커밋 ");
            text.slot(1);
            text.literal("을(를) 병합했습니다 ");
            text.slot(3);
        }
        TimelineEventMessage::MergedInto => {
            text.slot(0);
            text.literal(" 님이 커밋 ");
            text.slot(1);
            text.literal("을(를) ");
            text.slot(2);
            text.literal("에 병합했습니다 ");
            text.slot(3);
        }
        TimelineEventMessage::ReviewRequestedSelf => {
            text.slot(0);
            text.literal(" 님이 자신에게 리뷰를 요청했습니다");
        }
        TimelineEventMessage::ReviewRequested => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 님에게 리뷰를 요청했습니다");
        }
        TimelineEventMessage::ReviewRequestRemoved => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 님에 대한 리뷰 요청을 삭제했습니다");
        }
        TimelineEventMessage::ReviewDismissedOf => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 님의 ");
            text.slot(2);
            text.literal("를 기각했습니다");
        }
        TimelineEventMessage::ReviewDismissed => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(2);
            text.literal("를 기각했습니다");
        }
        TimelineEventMessage::ThreadResolved => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("를 해결했습니다");
        }
        TimelineEventMessage::ThreadReopened => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("를 다시 열었습니다");
        }
        TimelineEventMessage::HeadRefDeleted => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal(" 브랜치를 삭제했습니다");
        }
        TimelineEventMessage::ConvertedToDraft => {
            text.slot(0);
            text.literal(" 님이 이 풀 리퀘스트를 초안으로 표시했습니다");
        }
        TimelineEventMessage::ReadyForReview => {
            text.slot(0);
            text.literal(" 님이 이 풀 리퀘스트를 리뷰 준비 완료로 표시했습니다");
        }
    }
    TimelineNoteMessage => MessageWriter |text| {
        TimelineNoteMessage::Review => text.literal("리뷰"),
        TimelineNoteMessage::ReviewConversation => text.literal("리뷰 대화"),
        TimelineNoteMessage::MergeCommit => text.literal("(병합 커밋)"),
        TimelineNoteMessage::Squash => text.literal("(스쿼시)"),
        TimelineNoteMessage::Rebase => text.literal("(리베이스)"),
        TimelineNoteMessage::OutsideInstance => text.literal("(이 인스턴스 외부)"),
    }
}

/// How the issue or pull request was closed, ending the sentence.
const fn closed_as(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Completed => "완료됨으로 닫았습니다",
        CloseReason::NotPlanned => "계획 없음으로 닫았습니다",
        CloseReason::Merged => "병합됨으로 닫았습니다",
    }
}
