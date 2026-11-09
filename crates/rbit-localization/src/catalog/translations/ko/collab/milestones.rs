//! Korean (`ko`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("새 마일스톤"),
        MilestoneMessage::EditMilestone => text.literal("마일스톤 편집"),
        MilestoneMessage::EmptyTitle => text.literal("마일스톤이 없습니다"),
        MilestoneMessage::EmptyText => text.literal("마일스톤은 릴리스나 날짜를 목표로 이슈를 묶고 진행 상황을 보여 줍니다."),
        MilestoneMessage::ClosedWhen { when, .. } => {
            text.literal("닫힘: ");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due, .. } => {
            text.literal("마감일: ");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due, .. } => {
            text.literal("마감일 지남: ");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("마감일 없음"),
        MilestoneMessage::Close => text.literal("닫기"),
        MilestoneMessage::Reopen => text.literal("다시 열기"),
        MilestoneMessage::DeleteConfirm { title, .. } => {
            text.literal("마일스톤 “");
            text.argument(title);
            text.literal("”을(를) 삭제할까요? 이슈는 유지됩니다.");
        }
        MilestoneMessage::SearchIssues => text.literal("이 이슈 검색"),
        MilestoneMessage::NothingHereTitle => text.literal("항목이 없습니다"),
        MilestoneMessage::NothingHereText => text.literal("이 마일스톤에 추가된 이슈와 풀 리퀘스트가 여기에 표시됩니다."),
        MilestoneMessage::FormIntro => text.literal("릴리스나 날짜를 목표로 이슈를 묶습니다."),
        MilestoneMessage::DueDate => text.literal("마감일"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown, 선택 사항"),
        MilestoneMessage::CreateMilestone => text.literal("마일스톤 만들기"),
        MilestoneMessage::SaveMilestone => text.literal("마일스톤 저장"),
        MilestoneMessage::Complete { percent, .. } => {
            text.number(percent);
            text.literal("% 완료");
        }
        MilestoneMessage::OpenIssues { count, .. } => {
            text.number(count);
            text.literal("개 열림");
        }
        MilestoneMessage::ClosedIssues { count, .. } => {
            text.number(count);
            text.literal("개 닫힘");
        }
    }
}
