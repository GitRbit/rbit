//! Korean (`ko`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("담당자"),
        IssueSidebarMessage::NoOne => text.literal("없음"),
        IssueSidebarMessage::AssignYourself => text.literal("나를 담당자로 지정"),
        IssueSidebarMessage::EditAssignees => text.literal("담당자 편집"),
        IssueSidebarMessage::SaveAssignees => text.literal("담당자 저장"),
        IssueSidebarMessage::Labels => text.literal("레이블"),
        IssueSidebarMessage::NoneYet => text.literal("아직 없음"),
        IssueSidebarMessage::EditLabels => text.literal("레이블 편집"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("이 저장소에는 아직 레이블이 없습니다."),
        IssueSidebarMessage::SaveLabels => text.literal("레이블 저장"),
        IssueSidebarMessage::Milestone => text.literal("마일스톤"),
        IssueSidebarMessage::NoMilestone => text.literal("마일스톤 없음"),
        IssueSidebarMessage::ChangeMilestone => text.literal("마일스톤 변경"),
        IssueSidebarMessage::SaveMilestone => text.literal("마일스톤 저장"),
        IssueSidebarMessage::Notifications => text.literal("알림"),
        IssueSidebarMessage::Subscribe => text.literal("구독"),
        IssueSidebarMessage::Unsubscribe => text.literal("구독 취소"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("이 이슈에 대한 알림을 받고 있지 않습니다."),
        IssueSidebarMessage::NotNotifiedPull => text.literal("이 풀 리퀘스트에 대한 알림을 받고 있지 않습니다."),
        IssueSidebarMessage::Danger => text.literal("위험 구역"),
        IssueSidebarMessage::DeleteIssue => text.literal("이슈 삭제"),
        IssueSidebarMessage::DeletePull => text.literal("풀 리퀘스트 삭제"),
        IssueSidebarMessage::DeleteIssueConfirm => text.literal("이 이슈를 영구적으로 삭제할까요? 이 작업은 되돌릴 수 없습니다."),
        IssueSidebarMessage::DeletePullConfirm => text.literal("이 풀 리퀘스트를 영구적으로 삭제할까요? 이 작업은 되돌릴 수 없습니다."),
        IssueSidebarMessage::ReasonManual => text.literal("이 스레드를 구독했습니다."),
        IssueSidebarMessage::ReasonComment => text.literal("이 스레드에 댓글을 남겼습니다."),
        IssueSidebarMessage::ReasonReviewed => text.literal("이 풀 리퀘스트를 리뷰했습니다."),
        IssueSidebarMessage::ReasonAuthor => text.literal("이 스레드를 열었습니다."),
        IssueSidebarMessage::ReasonMention => text.literal("회원님이 멘션되었습니다."),
        IssueSidebarMessage::ReasonAssigned => text.literal("회원님이 담당자로 지정되었습니다."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("회원님에게 리뷰가 요청되었습니다."),
    }
}
