//! Japanese (`ja`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};

entries! { Ja:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle | AuthMessage::SignIn => text.literal("サインイン"),
        AuthMessage::SignUpTitle | AuthMessage::SignUp => text.literal("アカウント登録"),
        AuthMessage::SignInTo(site) => {
            text.argument(site);
            text.literal(" にサインイン");
        }
        AuthMessage::SignInLede => text.literal("ユーザー名またはメールアドレスを使用してください。"),
        AuthMessage::UsernameOrEmail => text.literal("ユーザー名またはメールアドレス"),
        AuthMessage::Password => text.literal("パスワード"),
        AuthMessage::ForgotPassword => text.literal("パスワードをお忘れですか？"),
        AuthMessage::Or => text.literal("または"),
        AuthMessage::SignInWith(provider) => {
            text.argument(provider);
            text.literal(" でサインイン");
        }
        AuthMessage::NoAccount => text.literal("アカウントをお持ちでないですか？"),
        AuthMessage::CreateYourAccount => text.literal("アカウントを作成"),
        AuthMessage::HostYourCode(site) => {
            text.argument(site);
            text.literal(" でコードをホストしましょう。");
        }
        AuthMessage::Username => text.literal("ユーザー名"),
        AuthMessage::UsernameHint => text.literal("英字、数字、「-」「_」「.」"),
        AuthMessage::Email => text.literal("メールアドレス"),
        AuthMessage::MinimumLength(count) => {
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" 文字以上。");
        }
        AuthMessage::SetupToken => text.literal("セットアップトークン"),
        AuthMessage::SetupTokenHint => {
            text.literal("最初のアカウントがこのサーバーの管理者になります。トークンはサーバーログに記録されています。");
        }
        AuthMessage::CreateAccount => text.literal("アカウントを作成"),
        AuthMessage::RegistrationClosed => {
            text.literal("このサーバーではアカウント登録を受け付けていません。管理者にアカウントの作成を依頼してください。");
        }
        AuthMessage::HaveAccount => text.literal("すでにアカウントをお持ちですか？"),
        AuthMessage::TwoFactorTitle => text.literal("2 要素認証"),
        AuthMessage::PasskeyOrApp => {
            text.literal("パスキーを使用するか、認証アプリに表示される 6 桁のコードを入力してください。");
        }
        AuthMessage::PasskeyOnly => {
            text.literal("パスキーまたはセキュリティキーを使用してサインインを完了してください。");
        }
        AuthMessage::AppOnly => text.literal("認証アプリに表示される 6 桁のコードを入力してください。"),
        AuthMessage::UsePasskey => text.literal("パスキーを使用"),
        AuthMessage::PasskeyUnsupported => {
            text.literal("このブラウザーではパスキーを使用できません。代わりにコードを入力してください。");
        }
        AuthMessage::AuthenticationCode => text.literal("認証コード"),
        AuthMessage::LostDevice => {
            text.literal("デバイスを紛失しましたか？代わりにリカバリーコードのいずれかを入力してください。");
        }
        AuthMessage::RecoveryCode => text.literal("リカバリーコード"),
        AuthMessage::LostPasskey => {
            text.literal("パスキーを紛失しましたか？リカバリーコードのいずれかを入力してください。");
        }
        AuthMessage::VerifyCode => text.literal("コードを確認"),
        AuthMessage::NotYou => text.literal("別のユーザーですか？"),
        AuthMessage::DifferentAccount => text.literal("別のアカウントでサインイン"),
        AuthMessage::ProviderLede(provider, site) => {
            text.argument(provider);
            text.literal(" でサインインしました。");
            text.argument(site);
            text.literal(" で使用するユーザー名を選んでください。");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.argument(provider);
            text.literal(" により確認済みです。別のアドレスを使う場合は、メールでの確認が必要です。");
        }
        AuthMessage::ConfirmByEmail => text.literal("このアドレスを確認するためのメールが届きます。"),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("今後は ");
            text.argument(provider);
            text.literal(
                " でサインインします。リセットリンクからパスワードを設定するまで、アカウントにパスワードはありません。HTTPS 経由で Git を使うには個人用アクセストークンが必要です。",
            );
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("し、セキュリティ設定で ");
            text.argument(provider);
            text.literal(" をリンクしてください。");
        }
        AuthMessage::SignInFailed => text.literal("サインインできませんでした"),
        AuthMessage::BackToSignIn => text.literal("サインインに戻る"),
        AuthMessage::BackToSecuritySettings => text.literal("セキュリティ設定に戻る"),
        AuthMessage::IncorrectCredentials => text.literal("ユーザー名またはパスワードが正しくありません。"),
        AuthMessage::Suspended => {
            text.literal("このアカウントは停止されています。管理者に問い合わせてください。");
        }
        AuthMessage::SignInExpired => {
            text.literal("サインインの有効期限が切れました。もう一度サインインしてください。");
        }
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "サインインの試行回数が多すぎます。",
                Attempt::SignUp => "アカウント登録の試行回数が多すぎます。",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal(" 秒後にもう一度お試しください。");
            } else {
                text.number(seconds.div_ceil(60));
                text.literal(" 分後にもう一度お試しください。");
            }
        }
    }
}
