//! Japanese (`ja`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("オープン"),
        ThreadMessage::Closed => text.literal("クローズ"),
        ThreadMessage::Merged => text.literal("マージ済み"),
        ThreadMessage::Draft => text.literal("ドラフト"),
        ThreadMessage::Title => text.literal("タイトル"),
        ThreadMessage::Description => text.literal("説明"),
        ThreadMessage::Edit => text.literal("編集"),
        ThreadMessage::Save => text.literal("保存"),
        ThreadMessage::Cancel => text.literal("キャンセル"),
        ThreadMessage::Delete => text.literal("削除"),
        ThreadMessage::Optional => text.literal("任意"),
        ThreadMessage::MarkdownSupported => text.literal("Markdown を使用できます"),
        ThreadMessage::SignIn => text.literal("サインイン"),
        ThreadMessage::LoadMore => text.literal("さらに表示"),
        ThreadMessage::AddComment => text.literal("コメントを追加"),
        ThreadMessage::CommentPlaceholder => text.literal("コメントを入力してください。@ でユーザー、# で Issue やプルリクエストを指定できます。"),
        ThreadMessage::MentionSuggestions => text.literal("メンションの候補"),
        ThreadMessage::CommentHint => text.literal("Markdown を使用できます。@name、#123、owner/repo#123 でユーザー、Issue、プルリクエストにリンクできます。"),
        ThreadMessage::LockedHint => text.literal("この会話はロックされています。コメントできるのはコラボレーターのみです。"),
        ThreadMessage::CloseIssue => text.literal("Issue をクローズ"),
        ThreadMessage::ClosePull => text.literal("プルリクエストをクローズ"),
        ThreadMessage::CloseNotPlanned => text.literal("計画なしとしてクローズ"),
        ThreadMessage::ReopenIssue => text.literal("Issue を再オープン"),
        ThreadMessage::ReopenPull => text.literal("プルリクエストを再オープン"),
        ThreadMessage::Comment => text.literal("コメント"),
        ThreadMessage::LockConversation => text.literal("会話をロック"),
        ThreadMessage::UnlockConversation => text.literal("会話のロックを解除"),
        ThreadMessage::Bot => text.literal("bot"),
        ThreadMessage::Edited => text.literal("編集済み"),
        ThreadMessage::Author => text.literal("作成者"),
        ThreadMessage::Reply => text.literal("返信"),
        ThreadMessage::DeleteCommentConfirm => text.literal("このコメントを削除しますか？"),
        ThreadMessage::DeleteComment => text.literal("コメントを削除"),
        ThreadMessage::NoDescription => text.literal("説明はありません。"),
    }
}
