//! Japanese (`ja`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::ChromeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("コンテンツへスキップ"),
        ChromeMessage::Main => text.literal("メイン"),
        ChromeMessage::Explore => text.literal("探索"),
        ChromeMessage::Language => text.literal("言語"),
        ChromeMessage::SwitchTheme => text.literal("ライトとダークを切り替え"),
        ChromeMessage::ChooseTheme => text.literal("テーマを選択"),
        ChromeMessage::Notifications => text.literal("通知"),
        ChromeMessage::Unread => text.literal("未読の通知があります"),
        ChromeMessage::NewRepository => text.literal("新しいリポジトリ"),
        ChromeMessage::AccountMenu => text.literal("アカウントメニュー"),
        ChromeMessage::Issues => text.literal("Issue"),
        ChromeMessage::Pulls => text.literal("プルリクエスト"),
        ChromeMessage::Dashboard => text.literal("ダッシュボード"),
        ChromeMessage::Profile => text.literal("プロフィール"),
        ChromeMessage::NewOrganization => text.literal("新しい組織"),
        ChromeMessage::Settings => text.literal("設定"),
        ChromeMessage::Administration => text.literal("サイト管理"),
        ChromeMessage::SignOut => text.literal("サインアウト"),
        ChromeMessage::SignIn => text.literal("サインイン"),
        ChromeMessage::SignUp => text.literal("アカウント登録"),
        ChromeMessage::Menu => text.literal("メニュー"),
        ChromeMessage::CreateAccount => text.literal("アカウントを作成"),
        ChromeMessage::Themes => text.literal("テーマ"),
        ChromeMessage::LightMode => text.literal("ライトモード"),
        ChromeMessage::DarkMode => text.literal("ダークモード"),
        ChromeMessage::PoweredBy => text.literal("提供："),
        ChromeMessage::UpdateAvailable => text.literal("アップデートがあります"),
        ChromeMessage::TimingHelp => text.literal("直前のページまたは更新にかかったサーバー時間"),
        ChromeMessage::RenderedIn => text.literal("描画時間"),
        ChromeMessage::Footer => text.literal("フッター"),
        ChromeMessage::Support => text.literal("ヘルプ"),
        ChromeMessage::Terms => text.literal("利用規約"),
        ChromeMessage::Privacy => text.literal("プライバシー"),
        ChromeMessage::Status => text.literal("ステータス"),
        ChromeMessage::Home { site } => {
            text.argument(site);
            text.literal(" ホーム");
        }
    }
}
