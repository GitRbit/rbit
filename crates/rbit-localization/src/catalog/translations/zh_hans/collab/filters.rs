//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

/// The menu's name.
const fn menu(menu: IssueFilterMenu) -> &'static str {
    match menu {
        IssueFilterMenu::State => "状态",
        IssueFilterMenu::Status => "进展",
        IssueFilterMenu::Author => "作者",
        IssueFilterMenu::Label => "标签",
        IssueFilterMenu::Milestone => "里程碑",
        IssueFilterMenu::Assignee => "受理人",
        IssueFilterMenu::Sort => "排序",
    }
}

entries! { ZhHans:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(name) => text.literal(menu(name)),
        IssueFilterMessage::Find(name) => {
            text.literal("查找");
            text.literal(menu(name));
        }
        IssueFilterMessage::FindPlaceholder(name) => {
            text.literal("查找");
            text.literal(menu(name));
            text.literal("…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("还没有可选项。"),
        IssueFilterMessage::NoMatches => text.literal("没有匹配项。"),
        IssueFilterMessage::Anyone => text.literal("任何人"),
        IssueFilterMessage::Nobody => text.literal("无人"),
        IssueFilterMessage::You => text.literal("你"),
        IssueFilterMessage::Any => text.literal("任意"),
        IssueFilterMessage::NoMilestone => text.literal("无里程碑"),
        IssueFilterMessage::AnyLabel => text.literal("任意标签"),
        IssueFilterMessage::NoLabels => text.literal("无标签"),
        IssueFilterMessage::Open => text.literal("开放"),
        IssueFilterMessage::Closed => text.literal("已关闭"),
        IssueFilterMessage::AllStates => text.literal("所有状态"),
        IssueFilterMessage::AnyStatus => text.literal("任意进展"),
        IssueFilterMessage::Draft => text.literal("草稿"),
        IssueFilterMessage::Merged => text.literal("已合并"),
        IssueFilterMessage::SortNewest => text.literal("最新"),
        IssueFilterMessage::SortOldest => text.literal("最早"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("最近更新"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("最久未更新"),
        IssueFilterMessage::SortMostCommented => text.literal("评论最多"),
    }
}
