//! Japanese (`ja`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::{IssueFilterMenu, IssueFilterMessage};
use crate::catalog::fallback::MessageWriter;

/// The menu's name, also the noun of its search box.
const fn menu_name(menu: IssueFilterMenu) -> &'static str {
    match menu {
        IssueFilterMenu::State => "状態",
        IssueFilterMenu::Status => "ステータス",
        IssueFilterMenu::Author => "作成者",
        IssueFilterMenu::Label => "ラベル",
        IssueFilterMenu::Milestone => "マイルストーン",
        IssueFilterMenu::Assignee => "担当者",
        IssueFilterMenu::Sort => "並べ替え",
    }
}

entries! { Ja:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(menu) => text.literal(menu_name(menu)),
        IssueFilterMessage::Find(menu) => {
            text.literal(menu_name(menu));
            text.literal("を検索");
        }
        IssueFilterMessage::FindPlaceholder(menu) => {
            text.literal(menu_name(menu));
            text.literal("を検索…");
        }
        IssueFilterMessage::NothingToChoose => text.literal("選択できる項目はまだありません。"),
        IssueFilterMessage::NoMatches => text.literal("一致する項目はありません。"),
        IssueFilterMessage::Anyone => text.literal("すべてのユーザー"),
        IssueFilterMessage::Nobody => text.literal("なし"),
        IssueFilterMessage::You => text.literal("自分"),
        IssueFilterMessage::Any => text.literal("すべて"),
        IssueFilterMessage::NoMilestone => text.literal("マイルストーンなし"),
        IssueFilterMessage::AnyLabel => text.literal("すべてのラベル"),
        IssueFilterMessage::NoLabels => text.literal("ラベルなし"),
        IssueFilterMessage::Open => text.literal("オープン"),
        IssueFilterMessage::Closed => text.literal("クローズ"),
        IssueFilterMessage::AllStates => text.literal("すべての状態"),
        IssueFilterMessage::AnyStatus => text.literal("すべてのステータス"),
        IssueFilterMessage::Draft => text.literal("ドラフト"),
        IssueFilterMessage::Merged => text.literal("マージ済み"),
        IssueFilterMessage::SortNewest => text.literal("新しい順"),
        IssueFilterMessage::SortOldest => text.literal("古い順"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("更新日が新しい順"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("更新日が古い順"),
        IssueFilterMessage::SortMostCommented => text.literal("コメントが多い順"),
    }
}
