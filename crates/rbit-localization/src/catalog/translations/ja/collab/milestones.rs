//! Japanese (`ja`) entries for `catalog/collab/milestones.rs`.

use crate::catalog::collab::milestones::MilestoneMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    MilestoneMessage<'a> => MessageWriter |text| {
        MilestoneMessage::NewMilestone => text.literal("新しいマイルストーン"),
        MilestoneMessage::EditMilestone => text.literal("マイルストーンを編集"),
        MilestoneMessage::EmptyTitle => text.literal("マイルストーンはありません"),
        MilestoneMessage::EmptyText => text.literal("マイルストーンは、リリースや期日に向けて Issue をまとめ、進捗を表示します。"),
        MilestoneMessage::ClosedWhen { when } => {
            text.literal("クローズ：");
            text.argument(when);
        }
        MilestoneMessage::DueBy { due } => {
            text.literal("期日：");
            text.argument(due);
        }
        MilestoneMessage::PastDueBy { due } => {
            text.literal("期日超過：");
            text.argument(due);
        }
        MilestoneMessage::NoDueDate => text.literal("期日なし"),
        MilestoneMessage::Close => text.literal("クローズ"),
        MilestoneMessage::Reopen => text.literal("再オープン"),
        MilestoneMessage::DeleteConfirm { title } => {
            text.literal("マイルストーン「");
            text.argument(title);
            text.literal("」を削除しますか？Issue は残ります。");
        }
        MilestoneMessage::SearchIssues => text.literal("これらの Issue を検索"),
        MilestoneMessage::NothingHereTitle => text.literal("項目はありません"),
        MilestoneMessage::NothingHereText => text.literal("このマイルストーンに追加された Issue やプルリクエストがここに表示されます。"),
        MilestoneMessage::FormIntro => text.literal("リリースや期日に向けて Issue をまとめます。"),
        MilestoneMessage::DueDate => text.literal("期日"),
        MilestoneMessage::MarkdownOptional => text.literal("Markdown（任意）"),
        MilestoneMessage::CreateMilestone => text.literal("マイルストーンを作成"),
        MilestoneMessage::SaveMilestone => text.literal("マイルストーンを保存"),
        MilestoneMessage::Complete { percent } => {
            text.number(percent);
            text.literal("% 完了");
        }
        MilestoneMessage::OpenIssues { count } => {
            text.literal("オープン ");
            text.number(count);
        }
        MilestoneMessage::ClosedIssues { count } => {
            text.literal("クローズ ");
            text.number(count);
        }
    }
}
