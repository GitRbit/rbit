//! Japanese (`ja`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::{InboxMessage, ThreadState};

entries! { Ja:
    InboxMessage<'a> => MessageWriter |text| {
        InboxMessage::Notifications => text.literal("通知"),
        InboxMessage::Show => text.literal("表示"),
        InboxMessage::Unread => text.literal("未読"),
        InboxMessage::All => text.literal("すべて"),
        InboxMessage::MarkAllRead => text.literal("すべて既読にする"),
        InboxMessage::NoNotifications => text.literal("通知はありません"),
        InboxMessage::NoNotificationsText => text.literal(
            "参加している Issue とプルリクエスト（自分が作成したもの、コメントしたもの、担当しているもの、メンションされたもの）の通知がここに届きます。",
        ),
        InboxMessage::AllCaughtUp => text.literal("すべて確認済みです"),
        InboxMessage::AllCaughtUpText => {
            text.literal("未読はありません。過去の通知は「すべて」にあります。");
        }
        InboxMessage::MarkAsRead => text.literal("既読にする"),
        InboxMessage::Unsubscribe => text.literal("購読解除"),
        InboxMessage::State(state) => text.literal(match state {
            ThreadState::OpenIssue => "オープンの Issue",
            ThreadState::ClosedIssue => "クローズされた Issue",
            ThreadState::OpenPull => "オープンのプルリクエスト",
            ThreadState::MergedPull => "マージ済みのプルリクエスト",
            ThreadState::ClosedPull => "クローズされたプルリクエスト",
        }),
        InboxMessage::StopEmails => text.literal("通知メールを停止"),
        InboxMessage::TurnOffFor(username) => {
            text.argument(username);
            text.literal(" の通知メールをオフにしますか？通知は引き続き受信トレイに表示されます。");
        }
        InboxMessage::TurnOff => text.literal("通知メールをオフにする"),
        InboxMessage::EmailsOff => text.literal("通知メールはオフになりました"),
        InboxMessage::NoLongerReceive(username) => {
            text.argument(username);
            text.literal(" は今後、通知メールを受信しません。いつでも再びオンにできます。");
        }
        InboxMessage::NotificationSettings => text.literal("通知設定"),
        InboxMessage::LinkNotValid => text.literal("このリンクは無効です"),
        InboxMessage::LinkNotValidText => text.literal(
            "リンクが不完全な可能性があります。サインインして、メールで受け取る通知を選択してください。",
        ),
    }
}
