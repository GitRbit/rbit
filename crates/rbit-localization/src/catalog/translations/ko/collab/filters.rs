//! Korean (`ko`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(menu_name(menu)),
        IssueFilterMessage::Find(menu) => {
            text.literal(menu_name(menu));
            text.literal(" 찾기");
        }
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal(menu_name(menu));
            text.literal(" 찾기…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("아직 선택할 항목이 없습니다."),
        IssueFilterMessage::NoMatches => text.literal("일치하는 항목이 없습니다."),
        IssueFilterMessage::Anyone => text.literal("모든 사용자"),
        IssueFilterMessage::Nobody => text.literal("없음"),
        IssueFilterMessage::You => text.literal("나"),
        IssueFilterMessage::Any => text.literal("전체"),
        IssueFilterMessage::NoMilestone => text.literal("마일스톤 없음"),
        IssueFilterMessage::AnyLabel => text.literal("모든 레이블"),
        IssueFilterMessage::NoLabels => text.literal("레이블 없음"),
        IssueFilterMessage::Open => text.literal("열림"),
        IssueFilterMessage::Closed => text.literal("닫힘"),
        IssueFilterMessage::AllStates => text.literal("모든 상태"),
        IssueFilterMessage::AnyStatus => text.literal("모든 진행 상태"),
        IssueFilterMessage::Draft => text.literal("초안"),
        IssueFilterMessage::Merged => text.literal("병합됨"),
        IssueFilterMessage::SortNewest => text.literal("최신순"),
        IssueFilterMessage::SortOldest => text.literal("오래된순"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("최근 업데이트순"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("업데이트 오래된순"),
        IssueFilterMessage::SortMostCommented => text.literal("댓글 많은순"),
    }
}

/// The Korean name of a filter menu.
const fn menu_name(menu: IssueFilterMenu) -> &'static str {
    match menu {
        IssueFilterMenu::State => "상태",
        IssueFilterMenu::Status => "진행 상태",
        IssueFilterMenu::Author => "작성자",
        IssueFilterMenu::Label => "레이블",
        IssueFilterMenu::Milestone => "마일스톤",
        IssueFilterMenu::Assignee => "담당자",
        IssueFilterMenu::Sort => "정렬",
    }
}
