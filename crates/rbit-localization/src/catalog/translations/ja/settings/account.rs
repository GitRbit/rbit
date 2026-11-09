//! Japanese (`ja`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { Ja:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("パスワードを更新してください"),
        AccountMessage::Title => text.literal("アカウント"),
        AccountMessage::Intro => text.literal("サインイン方法の設定です。変更には現在のパスワードが必要です。"),
        AccountMessage::Username => text.literal("ユーザー名"),
        AccountMessage::UsernameUpdated => text.literal("ユーザー名を更新しました。"),
        AccountMessage::UsernameHint => text.literal("古いユーザー名を使うプロフィールのリンクと Git のリモートは、誰かがその名前を取得するまで引き続き機能します。リンクとリモートを更新してください。サインインには新しいユーザー名かメールアドレスを使用してください。"),
        AccountMessage::CurrentPassword => text.literal("現在のパスワード"),
        AccountMessage::ChangeUsername => text.literal("ユーザー名を変更"),
        AccountMessage::VerificationSent => text.literal("確認メールを送信しました。受信トレイを確認してください。"),
        AccountMessage::Verified => text.literal("確認済み"),
        AccountMessage::Unverified => text.literal("未確認"),
        AccountMessage::VerifiedText => text.literal("このアドレスがあなたのものであることを確認済みです。"),
        AccountMessage::PendingText => text.literal("お送りしたメールのリンクからアドレスを確認してください。"),
        AccountMessage::UnavailableText => text.literal("このサーバーではメールを利用できないため、アドレスを確認できません。"),
        AccountMessage::Resend => text.literal("確認メールを再送信"),
        AccountMessage::EmailAddress => text.literal("メールアドレス"),
        AccountMessage::EmailUpdated => text.literal("メールアドレスを更新しました。"),
        AccountMessage::Email => text.literal("メールアドレス"),
        AccountMessage::EmailHintLead => text.literal("このアドレスまたはユーザー名でサインインできます。現在のアドレス："),
        AccountMessage::EmailHintTail => text.literal(""),
        AccountMessage::UpdateEmail => text.literal("メールアドレスを更新"),
        AccountMessage::Password => text.literal("パスワード"),
        AccountMessage::PasswordChanged => text.literal("パスワードを変更しました。"),
        AccountMessage::NewPassword => text.literal("新しいパスワード"),
        AccountMessage::ConfirmPassword => text.literal("新しいパスワード（確認）"),
        AccountMessage::ChangePassword => text.literal("パスワードを変更"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal("管理者が初期パスワードであなたのアカウントを作成しました。");
            text.argument(site);
            text.literal(" を続けて利用する前に、新しいパスワードを設定してください。");
        }
        AccountText::PasswordHint(length) => {
            text.number(length);
            text.literal(" 文字以上。関連のない単語をいくつか組み合わせると、強力で覚えやすいパスワードになります。");
        }
    }
}
