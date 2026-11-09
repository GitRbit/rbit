//! Korean (`ko`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("이슈"),
        IssueMessage::Labels => text.literal("레이블"),
        IssueMessage::Milestones => text.literal("마일스톤"),
        IssueMessage::NewIssue => text.literal("새 이슈"),
        IssueMessage::SearchIssues => text.literal("이슈 검색"),
        IssueMessage::SearchPlaceholder => text.literal("제목과 설명 검색…"),
        IssueMessage::Search => text.literal("검색"),
        IssueMessage::SearchHelp => text.literal("검색 도움말"),
        IssueMessage::ClearSearch => text.literal("검색, 필터, 정렬 초기화"),
        IssueMessage::StateNav => text.literal("상태"),
        IssueMessage::NoIssuesTitle => text.literal("일치하는 이슈가 없습니다"),
        IssueMessage::NoIssuesText => text.literal("다른 검색어를 시도하거나 첫 이슈를 여세요."),
        IssueMessage::DescriptionHint => text.literal("Markdown 지원, #12와 @name은 링크로 연결됩니다"),
        IssueMessage::NoLabelsYet => text.literal("아직 레이블이 없습니다."),
        IssueMessage::Assignees => text.literal("담당자"),
        IssueMessage::Milestone => text.literal("마일스톤"),
        IssueMessage::NoMilestone => text.literal("마일스톤 없음"),
        IssueMessage::OpenIssue => text.literal("이슈 열기"),
        IssueMessage::Details => text.literal("세부 정보"),
        IssueMessage::Locked => text.literal("잠김"),
        IssueMessage::AssignedTo { names, .. } => {
            text.literal("담당자: ");
            text.argument(names);
        }
        IssueMessage::Comments { count, .. } => {
            text.literal("댓글 ");
            text.number(count);
            text.literal("개");
        }
    }
}
