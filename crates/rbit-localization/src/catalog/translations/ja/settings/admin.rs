//! Japanese (`ja`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { Ja:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("サイト管理"),
        AdminMessage::Administration => text.literal("管理"),
        AdminMessage::Dashboard => text.literal("ダッシュボード"),
        AdminMessage::Users => text.literal("ユーザー"),
        AdminMessage::Organizations => text.literal("組織"),
        AdminMessage::Repositories => text.literal("リポジトリ"),
        AdminMessage::Settings => text.literal("設定"),
        AdminMessage::AuditLog => text.literal("監査ログ"),
        AdminMessage::Configuration => text.literal("構成"),
        AdminMessage::Runners => text.literal("ランナー"),
        AdminMessage::Search => text.literal("検索"),
        AdminMessage::Pagination => text.literal("ページ送り"),
        AdminMessage::Previous => text.literal("前へ"),
        AdminMessage::Next => text.literal("次へ"),
        AdminMessage::NoResults => text.literal("結果はありません"),
        AdminMessage::Cancel => text.literal("キャンセル"),
        AdminMessage::Actions => text.literal("操作"),
        AdminMessage::Private => text.literal("プライベート"),
        AdminMessage::Archived => text.literal("アーカイブ済み"),
        AdminMessage::Delete => text.literal("削除"),
        AdminMessage::TypeToConfirmLead => text.literal("確認のため"),
        AdminMessage::TypeToConfirmTail => text.literal("と入力してください"),
        AdminMessage::NewPassword => text.literal("新しいパスワード"),
        AdminMessage::ConfirmPassword => text.literal("新しいパスワード（確認）"),
        AdminMessage::Unavailable => text.literal("利用できません"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("このインスタンスの概要です。"),
        DashboardMessage::RestartServer => text.literal("サーバーを再起動"),
        DashboardMessage::RestartUnavailableHint => text.literal("このサーバーホストでは再起動を利用できません"),
        DashboardMessage::SoftwareUpdates => text.literal("ソフトウェア更新"),
        DashboardMessage::ConfigureUpdates => text.literal("更新の確認とバージョンのプライバシーを設定"),
        DashboardMessage::LiveMonitoring => text.literal("ライブ監視"),
        DashboardMessage::MonitoringIntro => text.literal("直近 5 分間のトラフィック、応答時間、Git のアクティビティです。"),
        DashboardMessage::Snapshot => text.literal("スナップショット"),
        DashboardMessage::Pause => text.literal("一時停止"),
        DashboardMessage::NeedsJavaScript => text.literal("ライブ更新には JavaScript が必要です。最新のスナップショットを見るには、このページを再読み込みしてください。"),
        DashboardMessage::System => text.literal("システム"),
        DashboardMessage::InstanceSettings => text.literal("インスタンス設定"),
        DashboardMessage::Edit => text.literal("編集"),
        DashboardMessage::Users => text.literal("ユーザー"),
        DashboardMessage::Organizations => text.literal("組織"),
        DashboardMessage::SharedNamespaces => text.literal("共有の名前空間"),
        DashboardMessage::Repositories => text.literal("リポジトリ"),
        DashboardMessage::AccessTokens => text.literal("アクセストークン"),
        DashboardMessage::AcrossAllUsers => text.literal("全ユーザーの合計"),
        DashboardMessage::Uptime => text.literal("稼働時間"),
        DashboardMessage::DataDirectory => text.literal("データディレクトリ"),
        DashboardMessage::DiskUsage => text.literal("ディスク使用量"),
        DashboardMessage::SiteName => text.literal("サイト名"),
        DashboardMessage::Explore => text.literal("探索ページ"),
        DashboardMessage::ExploreEnabled => text.literal("オン"),
        DashboardMessage::ExploreDisabled => text.literal("オフ"),
        DashboardMessage::Browsing => text.literal("閲覧"),
        DashboardMessage::BrowsingOpen => text.literal("サインインの有無を問わず誰でも"),
        DashboardMessage::BrowsingSignedIn => text.literal("サインインしたユーザーのみ"),
        DashboardMessage::Registration => text.literal("アカウント登録"),
        DashboardMessage::RegistrationOpen => text.literal("誰でも登録可能"),
        DashboardMessage::RegistrationClosed => text.literal("停止中（管理者がアカウントを作成）"),
        DashboardMessage::OrgPolicy => text.literal("組織"),
        DashboardMessage::AnyUserCreates => text.literal("すべてのユーザーが作成可能"),
        DashboardMessage::AdminsCreate => text.literal("管理者のみ作成可能"),
        DashboardMessage::NewRepositories => text.literal("新しいリポジトリ"),
        DashboardMessage::PublicByDefault => text.literal("デフォルトでパブリック"),
        DashboardMessage::PrivateByDefault => text.literal("デフォルトでプライベート"),
        DashboardMessage::EmailDigests => text.literal("メールダイジェスト"),
        DashboardMessage::Restarting => text.literal("サーバーを再起動しています"),
        DashboardMessage::RestartRequested => text.literal("再起動を要求しました"),
        DashboardMessage::RestartingText => text.literal("サーバーは再起動の前に、処理中の HTTP リクエストを完了し、SSH とバックグラウンドジョブを停止して、ストレージを閉じています。時間のかかる転送は完了までしばらくかかることがあります。"),
        DashboardMessage::Waiting => text.literal("サーバーの復帰を待っています…"),
        DashboardMessage::ReturnToDashboard => text.literal("ダッシュボードに戻る"),
        DashboardMessage::WaitNoScript => text.literal("しばらく待ってから、ダッシュボードに戻って再接続してください。"),
        DashboardMessage::RestartQuestion => text.literal("この rbit アプリケーションを再起動しますか？"),
        DashboardMessage::RestartText => text.literal("アプリケーションが終了し、保存された構成で再び起動するまでの間、サイトは一時的に利用できなくなります。HTTP リクエストは完了まで処理されますが、SSH 接続は切断されます。ユーザーの保存済みセッションは維持されます。"),
        DashboardMessage::RestartAudited => text.literal("この操作は管理の監査ログに記録されます。"),
        DashboardMessage::RestartUnavailable => text.literal("再起動を利用できません"),
        DashboardMessage::RestartUnsupported => text.literal("このサーバーホストはアプリケーションの再起動に対応していません。サービスマネージャーを使って rbit を再起動してください。"),
        DashboardMessage::BackToDashboard => text.literal("ダッシュボードに戻る"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("期間内のリクエスト"),
        MonitoringMessage::ErrorRate => text.literal("サーバーエラー率"),
        MonitoringMessage::ProcessUptime => text.literal("プロセスの稼働時間"),
        MonitoringMessage::Samples => text.literal("5 秒ごとのサンプル"),
        MonitoringMessage::FiveMinutesAgo => text.literal("5 分前"),
        MonitoringMessage::LatestSample => text.literal("最新のサンプル"),
        MonitoringMessage::Collecting => text.literal("最初のサンプルを収集しています。グラフは 5 秒後に表示されます。"),
        MonitoringMessage::HttpResponses => text.literal("HTTP レスポンス"),
        MonitoringMessage::CurrentWindow => text.literal("現在の期間"),
        MonitoringMessage::Status => text.literal("ステータス"),
        MonitoringMessage::RequestsColumn => text.literal("リクエスト"),
        MonitoringMessage::Share => text.literal("割合"),
        MonitoringMessage::GitActivity => text.literal("Git のアクティビティ"),
        MonitoringMessage::Operation => text.literal("操作"),
        MonitoringMessage::Exchanges => text.literal("通信数"),
        MonitoringMessage::CloneFetch => text.literal("クローン／フェッチ"),
        MonitoringMessage::Push => text.literal("プッシュ"),
        MonitoringMessage::GitNote => text.literal("失敗を含む Smart HTTP の通信です。SSH と参照の通知は含みません。応答時間はヘッダーまでの時間で、ストリーミング転送はその後も続く場合があります。"),
        MonitoringMessage::Footnote => text.literal("このプロセスのみが対象で、履歴は再起動時にリセットされます。監視のリクエストは含みません。"),
        MonitoringMessage::Informational => text.literal("情報"),
        MonitoringMessage::Successful => text.literal("成功"),
        MonitoringMessage::Redirects => text.literal("リダイレクト"),
        MonitoringMessage::ClientErrors => text.literal("クライアントエラー"),
        MonitoringMessage::ServerErrors => text.literal("サーバーエラー"),
        MonitoringMessage::RequestTraffic => text.literal("リクエストのトラフィック"),
        MonitoringMessage::ResponseTime => text.literal("応答時間"),
        MonitoringMessage::RequestsPerSecond => text.literal("件/秒"),
        MonitoringMessage::MillisecondsAverage => text.literal("ミリ秒（平均）"),
        MonitoringMessage::WaitingFirst => text.literal("最初のサンプルを待っています"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("rbit ");
            text.argument(version);
            text.literal(" のリリースノートを表示");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.literal("管理者 ");
            text.argument(admins);
            text.literal(" 人 · 停止中 ");
            text.argument(suspended);
            text.literal(" 人");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("管理者 1 人 · 停止中 ");
            text.argument(suspended);
            text.literal(" 人");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.literal("パブリック ");
            text.argument(public);
            text.literal(" · プライベート ");
            text.argument(private);
        }
        AdminText::Summary(first, last, total) => {
            text.argument(total);
            text.literal(" 件中 ");
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" 件");
        }
        AdminText::DigestsAt(hour) => {
            text.argument(hour);
            text.literal(" に送信（ユーザーが変更しない場合）");
        }
        AdminText::DeleteLabel(name) => {
            text.argument(name);
            text.literal(" を削除");
        }
        AdminText::ActionsFor(name) => {
            text.argument(name);
            text.literal(" の操作");
        }
        AdminText::PasswordLength(length) => {
            text.number(length);
            text.literal(" 文字以上。");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.literal("直近 5 分間の");
            text.argument(title);
            text.literal("。最新値：");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal("。リクエストがない間は応答時間が途切れます。");
        }
        AdminText::UpdatedAt(time) => {
            text.argument(time);
            text.literal(" UTC に更新");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(limit);
            text.literal(" 件中 ");
            text.number(samples);
            text.literal(" 件のサンプル");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · 管理");
        }
        AdminText::SetPasswordFor(name) => {
            text.argument(name);
            text.literal(" のパスワードを設定 · 管理");
        }
        AdminText::DeletePageTitle(name) => {
            text.argument(name);
            text.literal(" を削除 · 管理");
        }
    }
}
