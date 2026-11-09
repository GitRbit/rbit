//! Japanese (`ja`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(" 件の古いコメントが会話にあります");
        }
        ReviewMessage::CommentOnLine { line } => {
            text.number(line);
            text.literal(" 行目にコメント");
        }
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("解決済みの会話 · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::PendingCount { count } => {
            text.literal("保留中 ");
            text.number(count);
            text.literal(" 件");
        }
        ReviewMessage::DismissedBy { by, message } => {
            text.argument(by);
            text.literal(" が却下：");
            text.argument(message);
        }
        ReviewMessage::RemoveRequestFor { name } => {
            text.argument(name);
            text.literal(" へのレビューリクエストを取り消す");
        }
        ReviewMessage::Changes => text.literal("変更"),
        ReviewMessage::ToggleFileTree => text.literal("ファイルツリーの表示を切り替え"),
        ReviewMessage::CollapseAll => text.literal("すべて折りたたむ"),
        ReviewMessage::ExpandAll => text.literal("すべて展開"),
        ReviewMessage::FinishReview => text.literal("レビューを完了"),
        ReviewMessage::Truncated => text.literal("この変更は非常に大きいため、一部の差分は表示されません。すべての変更を確認するには、リポジトリをクローンしてください。"),
        ReviewMessage::FileToggleHint => text.literal("クリックで折りたたみ／展開 | Alt+クリックですべてのファイル"),
        ReviewMessage::ViewFile => text.literal("ファイルを表示"),
        ReviewMessage::StartReview => text.literal("レビューを開始"),
        ReviewMessage::AddSingleComment => text.literal("単一のコメントを追加"),
        ReviewMessage::BinaryFile => text.literal("バイナリファイルは表示されません。"),
        ReviewMessage::TooLarge => text.literal("差分が大きすぎるため表示できません。"),
        ReviewMessage::SubmoduleCommit => text.literal("サブモジュールのコミット"),
        ReviewMessage::NoContentChanges => text.literal("内容の変更はありません。"),
        ReviewMessage::NoChangesTitle => text.literal("変更はありません"),
        ReviewMessage::NoChangesText => text.literal("ヘッドにはベースへの変更がありません。"),
        ReviewMessage::Added => text.literal("追加"),
        ReviewMessage::Deleted => text.literal("削除"),
        ReviewMessage::Modified => text.literal("変更"),
        ReviewMessage::Renamed => text.literal("名前変更"),
        ReviewMessage::Copied => text.literal("コピー"),
        ReviewMessage::Outdated => text.literal("古い"),
        ReviewMessage::Pending => text.literal("保留中"),
        ReviewMessage::ReplyPlaceholder => text.literal("返信…"),
        ReviewMessage::AddToReview => text.literal("レビューに追加"),
        ReviewMessage::ReopenConversation => text.literal("会話を再オープン"),
        ReviewMessage::ResolveConversation => text.literal("会話を解決"),
        ReviewMessage::ReviewChanges => text.literal("変更をレビュー"),
        ReviewMessage::ReviewPlaceholder => text.literal("コメントを入力（Markdown を使用できます）"),
        ReviewMessage::RequestChanges => text.literal("変更をリクエスト"),
        ReviewMessage::Approve => text.literal("承認"),
        ReviewMessage::DiscardReview => text.literal("保留中のレビューを破棄"),
        ReviewMessage::DismissReview => text.literal("レビューを却下"),
        ReviewMessage::DismissReason => text.literal("このレビューを却下する理由は何ですか？"),
        ReviewMessage::Dismiss => text.literal("却下"),
        ReviewMessage::Reviewers => text.literal("レビュアー"),
        ReviewMessage::NoReviews => text.literal("レビューはまだありません"),
        ReviewMessage::RemoveRequest => text.literal("リクエストを取り消す"),
        ReviewMessage::RequestReviewFrom => text.literal("レビューをリクエストする相手"),
        ReviewMessage::Username => text.literal("ユーザー名"),
        ReviewMessage::RequestReview => text.literal("レビューをリクエスト"),
        ReviewMessage::Approved => text.literal("承認済み"),
        ReviewMessage::ChangesRequested => text.literal("変更をリクエスト済み"),
        ReviewMessage::Dismissed => text.literal("却下済み"),
        ReviewMessage::Commented => text.literal("コメント済み"),
        ReviewMessage::AwaitingReview => text.literal("レビュー待ち"),
    }
}
