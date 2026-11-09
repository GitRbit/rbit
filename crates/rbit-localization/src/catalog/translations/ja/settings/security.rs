//! Japanese (`ja`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Ja:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("セキュリティ"),
        SecurityMessage::Intro => text.literal("パスキーや認証アプリでアカウントを保護し、サインインしている場所を確認できます。"),
        SecurityMessage::TwoFactor => text.literal("2 要素認証"),
        SecurityMessage::On => text.literal("オン"),
        SecurityMessage::Off => text.literal("オフ"),
        SecurityMessage::EnabledLead => text.literal("サインイン時、パスワードの後にパスキーまたはコードを求めます。HTTPS 経由の Git には"),
        SecurityMessage::TokenLink => text.literal("個人用アクセストークン"),
        SecurityMessage::EnabledTail => text.literal("が必要です。パスワードはそこでは使えなくなります。"),
        SecurityMessage::Disabled => text.literal("サインインに 2 つ目のステップ（パスキー、認証アプリ、またはその両方）を追加します。"),
        SecurityMessage::Passkeys => text.literal("パスキーとセキュリティキー"),
        SecurityMessage::PasskeysText => text.literal("指紋、顔、デバイスの PIN、スマートフォン、または YubiKey などのハードウェアキーでサインインを確認します。"),
        SecurityMessage::Remove => text.literal("削除"),
        SecurityMessage::OnlyFactor => text.literal("これは唯一の 2 要素目です。削除すると 2 要素認証がオフになり、リカバリーコードも削除されます。"),
        SecurityMessage::PasskeyGone => text.literal("このパスキーではサインインできなくなります。"),
        SecurityMessage::Name => text.literal("名前"),
        SecurityMessage::PasskeyPlaceholder => text.literal("例：MacBook、YubiKey"),
        SecurityMessage::AddPasskey => text.literal("パスキーを追加"),
        SecurityMessage::PasskeyUnsupported => text.literal("パスキーを追加するには、パスキーに対応し、JavaScript が有効なブラウザーが必要です。"),
        SecurityMessage::PasskeyUrlLead => text.literal("パスキーを使うには、"),
        SecurityMessage::PasskeyUrlTail => text.literal("をこのサイトのアドレスに設定する必要があります。"),
        SecurityMessage::Authenticator => text.literal("認証アプリ"),
        SecurityMessage::AuthenticatorText => text.literal("1Password、Aegis、Google Authenticator などのアプリが生成する 6 桁のコードを使います。"),
        SecurityMessage::SetUp => text.literal("設定"),
        SecurityMessage::RemoveAuthenticator => text.literal("認証アプリを削除"),
        SecurityMessage::PasswordAlone => text.literal("アカウントはパスワードのみで保護されるようになり、リカバリーコードは削除されます。"),
        SecurityMessage::PasskeysStay => text.literal("パスキーは引き続き 2 要素目として使われます。"),
        SecurityMessage::RecoveryCodes => text.literal("リカバリーコード"),
        SecurityMessage::Regenerate => text.literal("新しいリカバリーコードを生成"),
        SecurityMessage::CodesStop => text.literal("現在のリカバリーコードは使えなくなります。"),
        SecurityMessage::GenerateCodes => text.literal("新しいコードを生成"),
        SecurityMessage::SignInMethods => text.literal("サインイン方法"),
        SecurityMessage::Password => text.literal("パスワード"),
        SecurityMessage::NoPassword => text.literal("パスワードなし"),
        SecurityMessage::WithPassword => text.literal("パスワードまたはリンクされたアカウントでサインインします。"),
        SecurityMessage::LinkedOnlyLead => text.literal("アカウントにパスワードがありません。リンクされたアカウントでサインインしています。パスワードを設定するには、"),
        SecurityMessage::ResetLink => text.literal("パスワードリセットのリンク"),
        SecurityMessage::LinkedOnlyMiddle => text.literal("をリクエストしてください。HTTPS 経由の Git には"),
        SecurityMessage::LinkedOnlyTail => text.literal("が必要です。"),
        SecurityMessage::NoLongerAvailable => text.literal("利用できなくなりました"),
        SecurityMessage::Unlink => text.literal("リンク解除"),
        SecurityMessage::OnlyWay => text.literal("唯一のサインイン方法"),
        SecurityMessage::NotLinked => text.literal("未リンク"),
        SecurityMessage::Link => text.literal("リンク"),
        SecurityMessage::Sessions => text.literal("セッション"),
        SecurityMessage::SignOutOthers => text.literal("他のすべての場所からサインアウト"),
        SecurityMessage::ThisDevice => text.literal("このデバイス"),
        SecurityMessage::SignOut => text.literal("サインアウト"),
        SecurityMessage::RecentActivity => text.literal("最近のアクティビティ"),
        SecurityMessage::NoActivity => text.literal("アクティビティはまだありません"),
        SecurityMessage::NoActivityText => text.literal("アカウントのサインインやセキュリティの変更がここに表示されます。"),
        SecurityMessage::SetupTitle => text.literal("認証アプリを設定"),
        SecurityMessage::SetupIntro => text.literal("認証アプリで QR コードをスキャンし、表示された 6 桁のコードを入力してください。"),
        SecurityMessage::CantScan => text.literal("スキャンできない場合は、代わりにこのキーを入力してください"),
        SecurityMessage::CopyKey => text.literal("キーをコピー"),
        SecurityMessage::Copy => text.literal("コピー"),
        SecurityMessage::TimeBased => text.literal("時間ベース、6 桁、30 秒ごとに更新。"),
        SecurityMessage::ConfirmCode => text.literal("2. コードを確認"),
        SecurityMessage::AuthenticationCode => text.literal("認証コード"),
        SecurityMessage::Cancel => text.literal("キャンセル"),
        SecurityMessage::Enable => text.literal("有効にする"),
        SecurityMessage::TwoFactorOn => text.literal("2 要素認証がオンになりました。"),
        SecurityMessage::SaveCodes => text.literal("今すぐこれらのコードを保存してください"),
        SecurityMessage::CodesText => text.literal("パスキーや認証アプリを失くした場合、各コードで 1 回ずつサインインできます。コードは再表示されないため、パスワードマネージャーなどの安全な場所に保管してください。"),
        SecurityMessage::SavedCodes => text.literal("コードを保存しました"),
        SecurityMessage::AppOrRecoveryCode => text.literal("認証コードまたはリカバリーコード"),
        SecurityMessage::RecoveryCode => text.literal("リカバリーコード"),
        SecurityMessage::PasskeyInstead => text.literal("コードの代わりに、パスキーのいずれかで確認することもできます。"),
        SecurityMessage::AuthenticatorRemoved => text.literal("認証アプリを削除しました。"),
        SecurityMessage::AuthenticatorAdded => text.literal("認証アプリを追加しました。"),
        SecurityMessage::PasskeyAdded => text.literal("パスキーを追加しました。"),
        SecurityMessage::PasskeyRemoved => text.literal("パスキーを削除しました。"),
        SecurityMessage::SessionRevoked => text.literal("セッションをサインアウトしました。"),
        SecurityMessage::OtherSessionsRevoked => text.literal("他のすべてのセッションからサインアウトしました。"),
        SecurityMessage::SetupCancelled => text.literal("2 要素認証の設定をキャンセルしました。"),
        SecurityMessage::IdentityLinked => text.literal("アカウントをリンクしました。今後はこのアカウントでサインインできます。"),
        SecurityMessage::IdentityUnlinked => text.literal("アカウントのリンクを解除しました。"),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. ");
            text.argument(site);
            text.literal(" をアプリに追加");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.argument(provider);
            text.literal(" のリンクを解除");
        }
        SecurityText::SignOutLabel(device) => {
            text.argument(device);
            text.literal(" からサインアウト");
        }
        SecurityText::WithPasskey(action) => {
            text.literal("パスキーで");
            text.argument(action);
        }
    }
}
