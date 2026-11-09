//! Japanese (`ja`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal("（スカッシュ時は ");
            text.argument(squash);
            text.literal("）");
        }
        MergeMessage::Approvals { have, need } => {
            text.literal("必須の承認 ");
            text.number(need);
            text.literal(" 件中 ");
            text.number(have);
            text.literal(" 件");
        }
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(" 人のレビュアーが変更をリクエストしました");
        }
        MergeMessage::RequiredPassed { count } => {
            text.literal("必須チェック ");
            text.number(count);
            text.literal(" 件が成功しました");
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.literal("必須チェック ");
            text.number(required);
            text.literal(" 件中 ");
            text.number(failing);
            text.literal(" 件が失敗しました");
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("必須チェック ");
            text.number(required);
            text.literal(" 件中 ");
            text.number(waiting);
            text.literal(" 件を待っています");
        }
        MergeMessage::Merge => text.literal("マージ"),
        MergeMessage::ConflictingFiles => text.literal("コンフリクトしているファイル"),
        MergeMessage::WorkInProgress => text.literal("このプルリクエストはまだ作業中です。"),
        MergeMessage::ReadyForReview => text.literal("レビュー可能にする"),
        MergeMessage::MergeMethod => text.literal("マージ方法"),
        MergeMessage::CreateMergeCommit => text.literal("マージコミットを作成"),
        MergeMessage::SquashAndMerge => text.literal("スカッシュしてマージ"),
        MergeMessage::RebaseAndMerge => text.literal("リベースしてマージ"),
        MergeMessage::EditMessage => text.literal("コミットメッセージを編集"),
        MergeMessage::Message => text.literal("メッセージ"),
        MergeMessage::DeleteHead => text.literal("マージ後にヘッドブランチを削除"),
        MergeMessage::Bypass => text.literal("ブランチ保護の要件を満たさずにマージする（監査ログに記録されます）"),
        MergeMessage::MergePull => text.literal("プルリクエストをマージ"),
        MergeMessage::WriteAccessOnly => text.literal("このリポジトリへの書き込み権限を持つユーザーのみマージできます。"),
        MergeMessage::PullOpen => text.literal("プルリクエストはオープンです"),
        MergeMessage::PullClosed => text.literal("プルリクエストはクローズされています"),
        MergeMessage::StillDraft => text.literal("このプルリクエストはまだドラフトです"),
        MergeMessage::HeadExists => text.literal("ヘッドブランチが存在します"),
        MergeMessage::HeadDeleted => text.literal("ヘッドブランチは削除されました"),
        MergeMessage::NoConflicts => text.literal("ベースブランチとのコンフリクトはありません"),
        MergeMessage::Conflicts => text.literal("このブランチには解決が必要なコンフリクトがあります"),
        MergeMessage::CheckingConflicts => text.literal("コンフリクトを確認しています…"),
        MergeMessage::MergeabilityError => text.literal("マージできるかどうかを確認できませんでした"),
        MergeMessage::NoChangesRequested => text.literal("変更のリクエストはありません"),
        MergeMessage::MayPush => text.literal("ベースブランチにプッシュできます"),
        MergeMessage::MayNotPush => text.literal("ベースブランチへのプッシュは許可されていません"),
        MergeMessage::NoChecks => text.literal("ステータスチェックはありません"),
        MergeMessage::ChecksPending => text.literal("一部のチェックがまだ完了していません"),
        MergeMessage::ChecksPassed => text.literal("すべてのチェックに成功しました"),
        MergeMessage::ChecksFailed => text.literal("一部のチェックが成功しませんでした"),
    }
}
