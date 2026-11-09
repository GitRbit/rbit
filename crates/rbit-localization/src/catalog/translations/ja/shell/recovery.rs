//! Japanese (`ja`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;

entries! { Ja:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("パスワードをリセット"),
        RecoveryMessage::ForgotLede => text.literal(
            "アカウントのメールアドレスを入力してください。新しいパスワードを設定するためのリンクをお送りします。",
        ),
        RecoveryMessage::ForgotWithoutEmail => text.literal(
            "このサーバーではメールを利用できないため、メールでパスワードをリセットすることはできません。管理者に新しいパスワードの設定を依頼してください。",
        ),
        RecoveryMessage::CheckEmail => text.literal("メールを確認してください"),
        RecoveryMessage::LinkSent => text.literal(
            "そのアドレスを使用しているアカウントがあれば、新しいパスワードを設定するためのリンクを送信しました。リンクは 1 回だけ使用でき、1 時間で期限切れになります。",
        ),
        RecoveryMessage::NothingArrived => {
            text.literal("メールが届きませんか？迷惑メールフォルダーを確認するか、");
        }
        RecoveryMessage::TryAgain => text.literal("もう一度お試しください"),
        RecoveryMessage::SendResetLink => text.literal("リセットリンクを送信"),
        RecoveryMessage::RememberedIt => text.literal("パスワードを思い出しましたか？"),
        RecoveryMessage::RequestNewLink => text.literal("新しいリンクをリクエスト"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal(
            "このサーバーではメールを利用できないため、新しいリンクを送信できません。管理者に新しいパスワードの設定を依頼してください。",
        ),
        RecoveryMessage::ChooseNewPassword => text.literal("新しいパスワードを設定"),
        RecoveryMessage::ResetFor(username) => {
            text.argument(username);
            text.literal(" のパスワードです。今後はこのパスワードでサインインします。");
        }
        RecoveryMessage::NewPassword => text.literal("新しいパスワード"),
        RecoveryMessage::ConfirmNewPassword => text.literal("新しいパスワード（確認）"),
        RecoveryMessage::ChangePassword => text.literal("パスワードを変更"),
        RecoveryMessage::BackToSignIn => text.literal("サインインに戻る"),
        RecoveryMessage::PasswordChanged => text.literal("パスワードを変更しました"),
        RecoveryMessage::PasswordChangedText => {
            text.literal("新しいパスワードを設定しました。次回のサインインから使用してください。");
        }
        RecoveryMessage::LinkExpired => text.literal("このリンクは期限切れです"),
        RecoveryMessage::ResetLinkExpiredText => text.literal(
            "パスワードリセットのリンクは 1 回だけ使用でき、1 時間で期限切れになります。続けるには新しいリンクをリクエストしてください。",
        ),
        RecoveryMessage::AccountSuspended => text.literal("このアカウントは停止されています"),
        RecoveryMessage::AccountSuspendedText => {
            text.literal("このアカウントのパスワードはリセットできません。管理者に問い合わせてください。");
        }
        RecoveryMessage::EmailVerified => text.literal("メールアドレスを確認しました"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.argument(address);
            text.literal(" をご確認いただきありがとうございます。");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal(
            "確認リンクは 1 回だけ使用でき、2 日で期限切れになります。また、アドレスを変更すると無効になります。サインインして、アカウント設定から新しいリンクを送信してください。",
        ),
        RecoveryMessage::GoToAccountSettings => text.literal("アカウント設定へ"),
        RecoveryMessage::SignIn => text.literal("サインイン"),
    }
}
