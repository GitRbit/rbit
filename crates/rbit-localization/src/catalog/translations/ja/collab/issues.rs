//! Japanese (`ja`) entries for `catalog/collab/issues.rs`.

use crate::catalog::collab::issues::IssueMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    IssueMessage<'a> => MessageWriter |text| {
        IssueMessage::Issues => text.literal("Issue"),
        IssueMessage::Labels => text.literal("ラベル"),
        IssueMessage::Milestones => text.literal("マイルストーン"),
        IssueMessage::NewIssue => text.literal("新しい Issue"),
        IssueMessage::SearchIssues => text.literal("Issue を検索"),
        IssueMessage::SearchPlaceholder => text.literal("タイトルと説明を検索…"),
        IssueMessage::Search => text.literal("検索"),
        IssueMessage::SearchHelp => text.literal("検索のヘルプ"),
        IssueMessage::ClearSearch => text.literal("検索、絞り込み、並べ替えをクリア"),
        IssueMessage::StateNav => text.literal("状態"),
        IssueMessage::NoIssuesTitle => text.literal("一致する Issue はありません"),
        IssueMessage::NoIssuesText => text.literal("別の条件で検索するか、最初の Issue を作成してください。"),
        IssueMessage::DescriptionHint => text.literal("Markdown 対応。#12 や @name はリンクになります"),
        IssueMessage::NoLabelsYet => text.literal("ラベルはまだありません。"),
        IssueMessage::Assignees => text.literal("担当者"),
        IssueMessage::Milestone => text.literal("マイルストーン"),
        IssueMessage::NoMilestone => text.literal("マイルストーンなし"),
        IssueMessage::OpenIssue => text.literal("Issue を作成"),
        IssueMessage::Details => text.literal("詳細"),
        IssueMessage::Locked => text.literal("ロック済み"),
        IssueMessage::AssignedTo { names } => {
            text.literal("担当者：");
            text.argument(names);
        }
        IssueMessage::Comments { count } => {
            text.number(count);
            text.literal(" 件のコメント");
        }
    }
}
