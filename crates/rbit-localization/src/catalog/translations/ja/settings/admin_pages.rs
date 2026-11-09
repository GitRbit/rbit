//! Japanese (`ja`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { Ja:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("このインスタンスにアカウントを持つすべてのユーザーです。"),
        AdminPageMessage::UsersSearch => text.literal("ユーザー名、名前、メールアドレスで検索"),
        AdminPageMessage::NewUser => text.literal("新しいユーザー"),
        AdminPageMessage::NoUsers => text.literal("ユーザーが見つかりません"),
        AdminPageMessage::NoUsersText => text.literal("別の条件で検索してください。"),
        AdminPageMessage::User => text.literal("ユーザー"),
        AdminPageMessage::Joined => text.literal("登録日"),
        AdminPageMessage::Admin => text.literal("管理者"),
        AdminPageMessage::Suspended => text.literal("停止中"),
        AdminPageMessage::EmailVerified => text.literal("メールアドレス確認済み"),
        AdminPageMessage::Verified => text.literal("確認済み"),
        AdminPageMessage::You => text.literal("あなた"),
        AdminPageMessage::RemoveAdmin => text.literal("管理者権限を外す"),
        AdminPageMessage::MakeAdmin => text.literal("管理者にする"),
        AdminPageMessage::LiftSuspension => text.literal("停止を解除"),
        AdminPageMessage::Suspend => text.literal("停止"),
        AdminPageMessage::SendReset => text.literal("パスワードリセットを送信"),
        AdminPageMessage::SetPasswordMenu => text.literal("パスワードを設定…"),
        AdminPageMessage::DeleteMenu => text.literal("削除…"),
        AdminPageMessage::ResetSent => text.literal("パスワードリセットのメールを送信しました。"),
        AdminPageMessage::PasswordSet => text.literal("パスワードを更新しました。"),
        AdminPageMessage::NewUserIntro => text.literal("アカウント登録が停止中でも、管理者はアカウントを作成できます。"),
        AdminPageMessage::Username => text.literal("ユーザー名"),
        AdminPageMessage::UsernameHint => text.literal("英字、数字、「-」「_」「.」"),
        AdminPageMessage::Email => text.literal("メールアドレス"),
        AdminPageMessage::InitialPassword => text.literal("初期パスワード"),
        AdminPageMessage::SiteAdministrator => text.literal("サイト管理者"),
        AdminPageMessage::SiteAdministratorText => text.literal("すべてのリポジトリとこの管理画面に完全にアクセスできます。"),
        AdminPageMessage::CreateUser => text.literal("ユーザーを作成"),
        AdminPageMessage::RegistrationClosed => text.literal("一般のアカウント登録は現在停止中のため、ユーザーを追加するにはこの方法しかありません。"),
        AdminPageMessage::SetPassword => text.literal("パスワードを設定"),
        AdminPageMessage::ResetHint => text.literal("ユーザー自身にパスワードを決めてもらう場合は、ユーザー一覧からパスワードリセットを送信してください。"),
        AdminPageMessage::DeleteUser => text.literal("ユーザーを削除"),
        AdminPageMessage::DeleteUserLead => text.literal("ユーザー"),
        AdminPageMessage::OrganizationsIntro => text.literal("共有の名前空間とそのメンバーです。"),
        AdminPageMessage::SearchByName => text.literal("名前で検索"),
        AdminPageMessage::NoOrganizations => text.literal("組織が見つかりません"),
        AdminPageMessage::NoOrganizationsText => text.literal("組織が作成されると、ここに表示されます。"),
        AdminPageMessage::Organization => text.literal("組織"),
        AdminPageMessage::Members => text.literal("メンバー"),
        AdminPageMessage::Created => text.literal("作成日"),
        AdminPageMessage::RepositoriesIntro => text.literal("パブリックとプライベートのすべてのリポジトリを、最近のアクティビティ順に表示します。"),
        AdminPageMessage::RepositoriesSearch => text.literal("オーナー/名前または説明で検索"),
        AdminPageMessage::NoRepositories => text.literal("リポジトリが見つかりません"),
        AdminPageMessage::NoRepositoriesText => text.literal("リポジトリが作成されると、ここに表示されます。"),
        AdminPageMessage::Repository => text.literal("リポジトリ"),
        AdminPageMessage::Updated => text.literal("更新日"),
        AdminPageMessage::DeleteRepository => text.literal("リポジトリを削除"),
        AdminPageMessage::DeleteRepositoryText => text.literal("すべての Git データを含め、リポジトリを完全に削除します。元に戻すことはできません。"),
        AdminPageMessage::AuditIntro => text.literal("サインイン、2 要素認証の変更、セッション、アクセストークン、管理者の操作を新しい順に表示します。"),
        AdminPageMessage::AuditSearch => text.literal("ユーザー、対象、IP アドレス"),
        AdminPageMessage::Category => text.literal("カテゴリー"),
        AdminPageMessage::Filter => text.literal("絞り込み"),
        AdminPageMessage::AllEvents => text.literal("すべてのイベント"),
        AdminPageMessage::Clear => text.literal("クリア"),
        AdminPageMessage::NoEvents => text.literal("イベントはありません"),
        AdminPageMessage::NoEventsText => text.literal("この条件に一致するイベントはまだありません。"),
        AdminPageMessage::As => text.literal("名義："),
        AdminPageMessage::By => text.literal("実行者："),
        AdminPageMessage::On => text.literal("対象："),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("変更はインスタンス全体にすぐに反映されます。"),
        InstanceSettingsMessage::Saved => text.literal("設定を保存しました。"),
        InstanceSettingsMessage::SiteName => text.literal("サイト名"),
        InstanceSettingsMessage::SiteNameHint => text.literal("ヘッダー、ページタイトル、アプリとしてインストールしたときに表示されます。"),
        InstanceSettingsMessage::Branding => text.literal("ブランディングとアイデンティティ"),
        InstanceSettingsMessage::BrandingHint => text.literal("テキストはプレーンテキストです。画像とリンクには HTTPS の URL か、リバースプロキシが配信するローカルの /パス を指定できます。画像の欄が空の場合は同梱のデフォルトを使用します。"),
        InstanceSettingsMessage::PrimaryColor => text.literal("プライマリボタンの色（空欄ならテーマの色）"),
        InstanceSettingsMessage::PrimaryTextColor => text.literal("プライマリボタンの文字色"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => text.literal("ボタンのラベルが読みやすいよう、十分なコントラストのある色を選んでください。両方の色を設定するか、両方とも空欄にしてください。"),
        InstanceSettingsMessage::Description => text.literal("サイトの説明"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("ウェルカムタイトル（空欄ならサイト名）"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("ウェルカムメッセージ"),
        InstanceSettingsMessage::LogoUrl => text.literal("ロゴの URL"),
        InstanceSettingsMessage::FaviconUrl => text.literal("ファビコンの URL"),
        InstanceSettingsMessage::AppIconUrl => text.literal("アプリアイコンの URL（正方形、512 ピクセル以上）"),
        InstanceSettingsMessage::FooterText => text.literal("フッターのテキスト"),
        InstanceSettingsMessage::SupportUrl => text.literal("サポートの URL"),
        InstanceSettingsMessage::TermsUrl => text.literal("利用規約の URL"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("プライバシーポリシーの URL"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("「Powered by rbit」を表示"),
        InstanceSettingsMessage::ShowVersion => text.literal("ソフトウェアのバージョンを表示"),
        InstanceSettingsMessage::ShowFeatures => text.literal("ウェルカムページにセルフホスティングの機能を表示"),
        InstanceSettingsMessage::Registration => text.literal("アカウント登録"),
        InstanceSettingsMessage::Open => text.literal("開放"),
        InstanceSettingsMessage::OpenText => text.literal("誰でもアカウント登録ページからアカウントを作成できます。"),
        InstanceSettingsMessage::Closed => text.literal("停止"),
        InstanceSettingsMessage::ClosedText => text.literal("管理者だけがアカウントを作成できます。"),
        InstanceSettingsMessage::OrgCreation => text.literal("組織を作成できるユーザー"),
        InstanceSettingsMessage::AnyUser => text.literal("すべてのユーザー"),
        InstanceSettingsMessage::AnyUserText => text.literal("サインインしているすべてのユーザーが組織を作成できます。"),
        InstanceSettingsMessage::AdminsOnly => text.literal("管理者のみ"),
        InstanceSettingsMessage::AdminsOnlyText => text.literal("既存の組織にはこれまでどおりユーザーを追加できます。"),
        InstanceSettingsMessage::DefaultVisibility => text.literal("リポジトリのデフォルトの公開範囲"),
        InstanceSettingsMessage::Public => text.literal("パブリック"),
        InstanceSettingsMessage::PublicText => text.literal("新しいリポジトリは、変更しない限り誰でも閲覧できます。"),
        InstanceSettingsMessage::Private => text.literal("プライベート"),
        InstanceSettingsMessage::PrivateText => text.literal("新しいリポジトリはプライベートで作成されます。オーナーは後からパブリックにできます。"),
        InstanceSettingsMessage::VisibilityHint => text.literal("新しいリポジトリのフォームで初期選択されます。"),
        InstanceSettingsMessage::DigestTime => text.literal("メールダイジェストの送信時刻"),
        InstanceSettingsMessage::DigestHint => text.literal("時刻を選んでいないユーザーに、毎日と毎週の通知ダイジェストを送信する時刻です。毎週のダイジェストは月曜日に送信されます。"),
        InstanceSettingsMessage::Save => text.literal("設定を保存"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.number(length);
            text.literal(" 文字以上。ユーザーには安全な方法で伝えてください。初回サインイン時に変更が必要です。");
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal("新しいパスワードは直ちに現在のパスワードに置き換わり、パスワードリセットのリンクはすべて無効になります。");
            text.argument(name);
            text.literal(" さんには安全な方法で伝えてください。");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal("、そのリポジトリ（");
            text.argument(repositories);
            text.literal("、すべての Git データを含む）、アクセストークン、組織のメンバーシップを完全に削除します。元に戻すことはできません。");
        }
    }
}
