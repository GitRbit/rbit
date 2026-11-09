//! Japanese (`ja`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};

entries! { Ja:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("パブリックリポジトリを検索"),
        HomeMessage::Search => text.literal("検索"),
        HomeMessage::ExploreAll => text.literal("すべてのリポジトリを見る"),
        HomeMessage::ThisInstance => text.literal("このインスタンス"),
        HomeMessage::RepositoryCount(_) => text.literal("リポジトリ"),
        HomeMessage::UserCount(_) => text.literal("ユーザー"),
        HomeMessage::OrganizationCount(_) => text.literal("組織"),
        HomeMessage::RecentlyUpdated => text.literal("最近の更新"),
        HomeMessage::ViewAll => text.literal("すべて表示"),
        HomeMessage::NoPublicRepositories => text.literal("パブリックリポジトリはまだありません"),
        HomeMessage::NoPublicRepositoriesText => {
            text.literal("このサーバーで公開されたリポジトリがここに表示されます。");
        }
        HomeMessage::FreshServer => {
            text.literal("このサーバーにはまだアカウントがありません。最初に登録したアカウントが管理者になります。");
        }
        HomeMessage::BuiltForSelfHosting => text.literal("セルフホスティングのために設計"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "高速な単一バイナリ",
            Feature::Protocols => "HTTPS と SSH での Git",
            Feature::Teams => "組織とチーム",
            Feature::Releases => "リリース、ミラー、Webhook",
            Feature::Security => "デフォルトで安全",
            Feature::Screens => "あらゆる画面で",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite と組み込みストアで動作します。ほかのサービスをインストールしたり実行したりする必要はありません。",
            Feature::Protocols => "プロトコル v2 対応の Smart HTTP、組み込みの SSH サーバー、デプロイキー。",
            Feature::Teams => "チームやコラボレーターとロールごとにリポジトリを共有できます。",
            Feature::Releases => "リリースを公開し、上流のプロジェクトをミラーし、ほかのサービスに通知できます。",
            Feature::Security => "2 要素サインイン、スコープ付きアクセストークン、監査ログ。",
            Feature::Screens => "スマートフォンの画面サイズまで対応し、アプリとしてインストールできます。",
        }),
        HomeMessage::Dashboard => text.literal("ダッシュボード"),
        HomeMessage::ViewProfile => text.literal("プロフィールを表示"),
        HomeMessage::New => text.literal("新規"),
        HomeMessage::NewRepository => text.literal("新しいリポジトリ"),
        HomeMessage::PersonalDashboards => text.literal("個人用ダッシュボード"),
        HomeMessage::Issues => text.literal("Issue"),
        HomeMessage::PullRequests => text.literal("プルリクエスト"),
        HomeMessage::Repositories => text.literal("リポジトリ"),
        HomeMessage::PrivateCount(count) => {
            text.literal("プライベート ");
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" 件");
        }
        HomeMessage::FindRepository => text.literal("リポジトリを検索…"),
        HomeMessage::FilterRepositories => text.literal("リポジトリを絞り込み"),
        HomeMessage::NoRepositories => text.literal("リポジトリはまだありません"),
        HomeMessage::NoRepositoriesText => {
            text.literal("リポジトリを作成して、このサーバーでコードのホスティングを始めましょう。");
        }
        HomeMessage::CreateRepository => text.literal("リポジトリを作成"),
        HomeMessage::NoFilterMatch => text.literal("条件に一致するリポジトリはありません。"),
        HomeMessage::Organizations => text.literal("組織"),
        HomeMessage::NewOrganization => text.literal("新しい組織"),
        HomeMessage::NotAMember => text.literal("どの組織のメンバーでもありません。"),
        HomeMessage::CreateOne => text.literal("組織を作成"),
        HomeMessage::ShareWithTeam => text.literal("すると、チームでリポジトリを共有できます。"),
        HomeMessage::Explore => text.literal("探索"),
        HomeMessage::NothingPublic => text.literal("公開されているものはまだありません。"),
        HomeMessage::YourInvolvement => text.literal("あなたの関与"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "自分が作成",
            Involvement::Assigned => "担当",
            Involvement::Mentioned => "メンション",
            Involvement::ReviewRequested => "レビュー依頼",
        }),
        HomeMessage::SearchIn(title) => {
            text.argument(title);
            text.literal(" を検索");
        }
        HomeMessage::SearchHelp => text.literal("検索のヘルプ"),
        HomeMessage::SearchHelpText => text.literal(
            "is:open、is:closed、author:@me、assignee:@me、label:bug、milestone:\"v1\"、no:assignee、sort:updated-desc を使用できます。テキストはタイトルと説明を検索します。結果には現在読み取りできるリポジトリのみが含まれます。「メンション」には、本文、コメント、送信済みのレビューでの既知のメンションが、通知を既読にした後も含まれます。",
        ),
        HomeMessage::Results => text.literal("結果"),
        HomeMessage::OpenCount(count) => {
            text.number(count);
            text.literal(" 件のオープン");
        }
        HomeMessage::ClosedCount(count) => {
            text.number(count);
            text.literal(" 件のクローズ");
        }
        HomeMessage::NoMatchingThreads => text.literal("一致するスレッドはありません"),
        HomeMessage::NoMatchingThreadsText => text.literal("別の絞り込み条件やタブをお試しください。"),
        HomeMessage::CommentCount(count) => {
            text.number(u64::from(count));
            text.literal(" 件のコメント");
        }
    }
}
