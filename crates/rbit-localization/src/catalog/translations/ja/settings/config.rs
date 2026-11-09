//! Japanese (`ja`) entries for `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares `write`, the field labels and help of one part of the schema, from arms
/// written like the table below so the coverage report counts them.
macro_rules! config_entries {
    (ConfigText<$lifetime:lifetime> => MessageWriter |$text:ident| { $($arms:tt)* }) => {
        /// Writes a field's label or help, or returns `false` when this part lacks it.
        #[allow(clippy::too_many_lines, reason = "one arm per field label and help")]
        pub(super) fn write(message: ConfigText<'_>, $text: &mut MessageWriter) -> bool {
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_a;
mod fields_b;

entries! { Ja:
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::SectionTitle("updates") => text.literal("ソフトウェア更新とプライバシー"),
        ConfigText::SectionTitle("server") => text.literal("サーバー"),
        ConfigText::SectionTitle("storage") => text.literal("ストレージ"),
        ConfigText::SectionTitle("security") => text.literal("セキュリティ"),
        ConfigText::SectionTitle("repository") => text.literal("リポジトリ"),
        ConfigText::SectionTitle("mail") => text.literal("メール配信"),
        ConfigText::SectionTitle("ssh") => text.literal("SSH"),
        ConfigText::SectionTitle("auth") => text.literal("サインインプロバイダー"),
        ConfigText::SectionTitle("branding") => text.literal("ブランディングのデフォルト値"),
        ConfigText::SectionTitle("notifications") => text.literal("メール通知"),
        ConfigText::SectionTitle("operations") => text.literal("運用"),
        ConfigText::SectionTitle("api") => text.literal("REST API"),
        ConfigText::SectionTitle("webhooks") => text.literal("Webhook"),
        ConfigText::SectionTitle("releases") => text.literal("リリース"),
        ConfigText::SectionTitle("mirrors") => text.literal("プルミラー"),
        ConfigText::SectionTitle("push_mirrors") => text.literal("プッシュミラー"),
        ConfigText::SectionTitle("imports") => text.literal("インポート"),
        ConfigText::SectionTitle("issues") => text.literal("Issue と受信トレイ"),
        ConfigText::SectionTitle("pulls") => text.literal("プルリクエスト"),
        ConfigText::SectionTitle("editor") => text.literal("ブラウザーエディター"),
        ConfigText::SectionTitle("lfs") => text.literal("Git LFS"),
        ConfigText::SectionTitle("actions") => text.literal("Actions"),
        ConfigText::SectionHelp("updates") => text.literal("リリースのバックグラウンド確認と、実行中のバージョンの公開に関する設定です。変更は再起動後に反映されます。"),
        ConfigText::SectionHelp("server") => text.literal("公開アドレス、待ち受けアドレス、インスタンスのデフォルト値です。"),
        ConfigText::SectionHelp("storage") => text.literal("データディレクトリを表示します。ストレージの移動は、デプロイ構成を使ってオフラインで行ってください。"),
        ConfigText::SectionHelp("security") => text.literal("セッション、パスワード、レート制限、信頼するリバースプロキシです。"),
        ConfigText::SectionHelp("repository") => text.literal("Git リポジトリのデフォルト値と上限です。"),
        ConfigText::SectionHelp("mail") => text.literal("送信者情報、SMTP 接続、配信のタイムアウトを構成します。"),
        ConfigText::SectionHelp("ssh") => text.literal("組み込みの Git SSH サーバー、公開クローンアドレス、接続数の上限です。"),
        ConfigText::SectionHelp("auth") => text.literal("OIDC と GitHub の ID プロバイダーを接続します。"),
        ConfigText::SectionHelp("branding") => text.literal("デプロイのデフォルト値です。インスタンス全体にすぐ反映する変更は「ブランディングとポリシー」で行ってください。"),
        ConfigText::SectionHelp("notifications") => text.literal("通知メールを有効にし、デフォルトの購読ポリシーを選択します。"),
        ConfigText::SectionHelp("operations") => text.literal("メトリクス、メンテナンス、バックグラウンドジョブ、キャッシュです。"),
        ConfigText::SectionHelp("api") => text.literal("API の提供とページングの上限です。"),
        ConfigText::SectionHelp("webhooks") => text.literal("送信、再試行、保持期間の設定です。"),
        ConfigText::SectionHelp("releases") => text.literal("リリースアセットのサイズ、数、アップロードのタイムアウトです。"),
        ConfigText::SectionHelp("mirrors") => text.literal("フェッチのスケジュール、タイムアウト、ネットワークアクセスです。"),
        ConfigText::SectionHelp("push_mirrors") => text.literal("プッシュのスケジュール、タイムアウト、ネットワークアクセスです。"),
        ConfigText::SectionHelp("imports") => text.literal("リポジトリのインポート、同時実行数、作成者の照合です。"),
        ConfigText::SectionHelp("issues") => text.literal("コミットのスキャンとアプリ内通知の保持期間です。"),
        ConfigText::SectionHelp("pulls") => text.literal("マージの上限、バックグラウンドチェック、作成者のアドレスです。"),
        ConfigText::SectionHelp("editor") => text.literal("ブラウザーからのアップロードのサイズ、ファイル数、タイムアウトの上限です。"),
        ConfigText::SectionHelp("lfs") => text.literal("大容量ファイルのストレージ、オブジェクトのサイズ、トークンの有効期間です。"),
        ConfigText::SectionHelp("actions") => text.literal("外部ランナーでの CI/CD、保持期間、上限、タイムアウトです。"),
        ConfigText::Choice { section: "mail", key: "transport", value: "disabled" } => text.literal("無効"),
        ConfigText::Choice { section: "mail", key: "transport", value: "log" } => text.literal("開発用ログ"),
        ConfigText::Choice { section: "mail", key: "transport", value: "smtp" } => text.literal("SMTP サーバー"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "starttls" } => text.literal("STARTTLS（推奨）"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "tls" } => text.literal("暗黙的 TLS"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "none" } => text.literal("暗号化なし"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "off" } => text.literal("オフ"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "participating" } => text.literal("参加しているスレッド"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "all" } => text.literal("購読しているすべてのスレッド"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "none" } => text.literal("元の作成者をテキストのまま保持"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email" } => text.literal("確認済みメールアドレスで照合"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email_or_username" } => text.literal("確認済みメールアドレスまたはユーザー名で照合"),
        ConfigText::Choice { section: "provider", key: "kind", value: "oidc" } => text.literal("OpenID Connect"),
        ConfigText::Choice { section: "provider", key: "kind", value: "github" } => text.literal("GitHub / GitHub Enterprise"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "true" } => text.literal("インスタンスの登録ポリシーに従う"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "false" } => text.literal("既存のアカウントのみ"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "always" } => text.literal("登録が締め切られていてもアカウント作成を許可"),
        field => return fields_a::write(field, text) || fields_b::write(field, text),
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("構成"),
        ConfigMessage::Intro => text.literal("機能ごとに整理された設定で、セルフホストのインスタンスを管理します。"),
        ConfigMessage::RestartServer => text.literal("サーバーを再起動"),
        ConfigMessage::Saved => text.literal("設定を保存しました。デプロイの変更を反映するには、サーバーを再起動してください。"),
        ConfigMessage::RestartRequired => text.literal("再起動が必要"),
        ConfigMessage::EnvironmentWins => text.literal("環境変数が優先されます。"),
        ConfigMessage::LivePolicies => text.literal("ブランディングとポリシー — すぐに反映"),
        ConfigMessage::SettingsSection => text.literal("設定セクション"),
        ConfigMessage::OpenSection => text.literal("セクションを開く"),
        ConfigMessage::Sections => text.literal("構成セクション"),
        ConfigMessage::LiveOverrides => text.literal("保存済みのブランディングと登録ポリシーは、これらのデプロイのデフォルト値より優先されます。"),
        ConfigMessage::IdentityProviders => text.literal("ID プロバイダー"),
        ConfigMessage::ProvidersHint => text.literal("先にプロバイダーカードを追加・削除してから、新しい認証情報を入力して保存してください。プロバイダーの変更は、セクションを保存するまでこのフォーム内にのみ保持されます。"),
        ConfigMessage::CallbackLead => text.literal("各プロバイダーにコールバック URL を登録してください。公開 URL の後に"),
        ConfigMessage::CallbackTail => text.literal(" を続け、PROVIDER_ID を下の ID に置き換えます。OpenID Connect では発行者 URL を、GitHub Enterprise では Enterprise の URL を設定してください。"),
        ConfigMessage::NoProviders => text.literal("外部サインインプロバイダーは構成されていません。ローカルのサインインを利用できます。"),
        ConfigMessage::RemoveProvider => text.literal("プロバイダーを削除"),
        ConfigMessage::AddProvider => text.literal("ID プロバイダーを追加"),
        ConfigMessage::NewProvider => text.literal("新しいサインインプロバイダー"),
        ConfigMessage::OnlySection => text.literal("このセクションのみ更新されます。"),
        ConfigMessage::Recovery => text.literal("復旧とデプロイのデフォルト値"),
        ConfigMessage::RecoveryLead => text.literal("保存した設定が原因でサーバーが起動しない場合は、サーバーの環境で"),
        ConfigMessage::RecoveryTail => text.literal("を設定して再起動し、ここで上書き設定を修正または削除してください。"),
        ConfigMessage::RemovingOverrides => text.literal("上書き設定を削除すると、次回の再起動時にファイルと環境変数の構成に戻ります。ブランディングとポリシーは保持されます。"),
        ConfigMessage::RemoveOverrides => text.literal("デプロイの上書き設定をすべて削除"),
        ConfigMessage::Optional => text.literal("（任意）"),
        ConfigMessage::OnePerLine => text.literal("1 行に 1 項目を入力します。"),
        ConfigMessage::ProviderDefaults => text.literal("プロバイダーのデフォルト値を使用"),
        ConfigMessage::Configured => text.literal("構成済み — 変更しない場合は空欄のまま"),
        ConfigMessage::NotConfigured => text.literal("未構成"),
        ConfigMessage::SecretHint => text.literal("現在の認証情報を維持するには空欄のままにしてください。新しい認証情報はエラー後に再表示されないため、フォームを修正するときに再入力してください。インスタンスのデータベースとそのバックアップを保護してください。"),
        ConfigMessage::ClearSecret => text.literal("この認証情報を消去"),
        ConfigMessage::BlankDefault => text.literal("デフォルト値を使用するには空欄のままにしてください。"),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("「");
            text.argument(section);
            text.literal("」の設定を保存");
        }
    }
}
