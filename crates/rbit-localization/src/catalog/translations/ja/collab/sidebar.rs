//! Japanese (`ja`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("担当者"),
        IssueSidebarMessage::NoOne => text.literal("なし"),
        IssueSidebarMessage::AssignYourself => text.literal("自分を割り当てる"),
        IssueSidebarMessage::EditAssignees => text.literal("担当者を編集"),
        IssueSidebarMessage::SaveAssignees => text.literal("担当者を保存"),
        IssueSidebarMessage::Labels => text.literal("ラベル"),
        IssueSidebarMessage::NoneYet => text.literal("まだありません"),
        IssueSidebarMessage::EditLabels => text.literal("ラベルを編集"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("このリポジトリにはまだラベルがありません。"),
        IssueSidebarMessage::SaveLabels => text.literal("ラベルを保存"),
        IssueSidebarMessage::Milestone => text.literal("マイルストーン"),
        IssueSidebarMessage::NoMilestone => text.literal("マイルストーンなし"),
        IssueSidebarMessage::ChangeMilestone => text.literal("マイルストーンを変更"),
        IssueSidebarMessage::SaveMilestone => text.literal("マイルストーンを保存"),
        IssueSidebarMessage::Notifications => text.literal("通知"),
        IssueSidebarMessage::Subscribe => text.literal("購読"),
        IssueSidebarMessage::Unsubscribe => text.literal("購読解除"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("この Issue の通知は受け取っていません。"),
        IssueSidebarMessage::NotNotifiedPull => text.literal("このプルリクエストの通知は受け取っていません。"),
        IssueSidebarMessage::Danger => text.literal("危険な操作"),
        IssueSidebarMessage::DeleteIssue => text.literal("Issue を削除"),
        IssueSidebarMessage::DeletePull => text.literal("プルリクエストを削除"),
        IssueSidebarMessage::DeleteIssueConfirm => text.literal("この Issue を完全に削除しますか？この操作は元に戻せません。"),
        IssueSidebarMessage::DeletePullConfirm => text.literal("このプルリクエストを完全に削除しますか？この操作は元に戻せません。"),
        IssueSidebarMessage::ReasonManual => text.literal("このスレッドを購読しています。"),
        IssueSidebarMessage::ReasonComment => text.literal("このスレッドにコメントしました。"),
        IssueSidebarMessage::ReasonReviewed => text.literal("このプルリクエストをレビューしました。"),
        IssueSidebarMessage::ReasonAuthor => text.literal("このスレッドを作成しました。"),
        IssueSidebarMessage::ReasonMention => text.literal("メンションされました。"),
        IssueSidebarMessage::ReasonAssigned => text.literal("担当者に割り当てられています。"),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("レビューをリクエストされました。"),
    }
}
