//! Japanese (`ja`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};

entries! { Ja:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("新しいリポジトリ"),
        RepoCreateLabel::RepositoryType => text.literal("リポジトリの種類"),
        RepoCreateLabel::Empty => text.literal("空"),
        RepoCreateLabel::Mirror => text.literal("ミラー"),
        RepoCreateLabel::Import => text.literal("インポート"),
        RepoCreateLabel::NewLead => text.literal("空のリポジトリを作成するか、プロジェクトをインポートするか、ソースと同期し続けるミラーを作成します。"),
        RepoCreateLabel::Owner => text.literal("オーナー"),
        RepoCreateLabel::Name => text.literal("名前"),
        RepoCreateLabel::Description => text.literal("説明"),
        RepoCreateLabel::Optional => text.literal("任意"),
        RepoCreateLabel::Visibility => text.literal("公開範囲"),
        RepoCreateLabel::Public => text.literal("パブリック"),
        RepoCreateLabel::Private => text.literal("プライベート"),
        RepoCreateLabel::PublicRepository => text.literal("誰でもこのリポジトリを閲覧、クローンできます。"),
        RepoCreateLabel::PublicMirror => text.literal("誰でもこのミラーを閲覧、クローンできます。"),
        RepoCreateLabel::PublicImport => text.literal("誰でもこのリポジトリを閲覧、クローンできます。"),
        RepoCreateLabel::PrivateHint => text.literal("オーナーだけが閲覧できます。組織の場合は、そのメンバーが閲覧できます。"),
        RepoCreateLabel::Cancel => text.literal("キャンセル"),
        RepoCreateLabel::CreateRepository => text.literal("リポジトリを作成"),
        RepoCreateLabel::MirrorLead => text.literal("ミラーは、外部のパブリックリポジトリのブランチとタグを定期的にフェッチします。ここでは読み取り専用で、変更はアップストリームから取り込まれます。"),
        RepoCreateLabel::MirrorsDisabled => text.literal("ミラーは無効です"),
        RepoCreateLabel::MirrorsDisabledText => text.literal("このインスタンスの管理者がプルミラーを無効にしています。"),
        RepoCreateLabel::UpstreamUrl => text.literal("アップストリームの URL"),
        RepoCreateLabel::UrlHintStart => text.literal(""),
        RepoCreateLabel::UrlHintOr => text.literal("または"),
        RepoCreateLabel::UrlHintEnd => text.literal("で始まるパブリックリポジトリの URL です。"),
        RepoCreateLabel::FromTheUrl => text.literal("URL から取得"),
        RepoCreateLabel::FetchEvery => text.literal("フェッチの間隔"),
        RepoCreateLabel::Minutes => text.literal("分"),
        RepoCreateLabel::CreateMirror => text.literal("ミラーを作成"),
        RepoCreateLabel::MirrorARepository => text.literal("リポジトリをミラー"),
        RepoCreateLabel::ImportLead => text.literal("GitHub または Gitea からリポジトリを履歴ごとコピーします。必要に応じて、Issue、プルリクエスト、ラベル、マイルストーン、リリース、Git LFS ファイルもコピーできます。Issue とプルリクエストの番号は変わりません。"),
        RepoCreateLabel::ImportingDisabled => text.literal("インポートは無効です"),
        RepoCreateLabel::ImportingDisabledText => text.literal("このインスタンスの管理者がインポートを無効にしています。"),
        RepoCreateLabel::ImportFrom => text.literal("インポート元"),
        RepoCreateLabel::ServerAddress => text.literal("サーバーのアドレス"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("github.com の場合は不要"),
        RepoCreateLabel::Repository => text.literal("リポジトリ"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("owner/name またはその URL"),
        RepoCreateLabel::AccessToken => text.literal("アクセストークン"),
        RepoCreateLabel::OptionalForPublic => text.literal("パブリックリポジトリの場合は任意"),
        RepoCreateLabel::TokenHint => text.literal("リポジトリを読み取れるトークンを指定すると、ソース側のレート制限が緩和されます。プライベートリポジトリの場合は必須です。トークンはインポート中にのみ使用され、保存されることはありません。"),
        RepoCreateLabel::AsOnTheSource => text.literal("ソースと同じ"),
        RepoCreateLabel::BringAlong => text.literal("一緒にインポートする項目"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("未対応：このインスタンスには Wiki がありません。"),
        RepoCreateLabel::StartImport => text.literal("インポートを開始"),
        RepoCreateLabel::ImportARepository => text.literal("リポジトリをインポート"),
        RepoCreateLabel::ImportTitle => text.literal("インポート"),
        RepoCreateLabel::FailedNote => text.literal("それまでにインポートされた内容はすべてリポジトリに残っています。"),
        RepoCreateLabel::NotReached => text.literal("未到達"),
        RepoCreateLabel::Started => text.literal("開始"),
        RepoCreateLabel::WaitingForSlot => text.literal("空きスロットを待機中"),
        RepoCreateLabel::Ended => text.literal("終了"),
        RepoCreateLabel::UpdatesByItself => text.literal("このページは自動的に更新されます。"),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("スキップまたは不完全な項目"),
        RepoCreateLabel::GoToRepository => text.literal("リポジトリへ移動"),
        RepoCreateLabel::OpensAutomatically => text.literal("最初のミラーフェッチが完了すると、リポジトリが自動的に開きます。"),
        RepoCreateLabel::FetchInProgress => text.literal("ミラーをフェッチしています"),
        RepoCreateLabel::TransferSizeNote => text.literal("転送サイズはアップストリームによって決まります。このページには、処理の進行に合わせて現在の段階が表示されます。"),
        RepoCreateLabel::RefreshStatus => text.literal("状態を更新"),
        RepoCreateLabel::SettingsAndRetry => text.literal("ミラーの設定と再試行"),
        RepoCreateLabel::NoScriptHint => text.literal("「状態を更新」で進行状況を確認し、準備ができたらリポジトリを開いてください。"),
        RepoCreateLabel::Connecting => text.literal("アップストリームのリポジトリに接続しています。"),
        RepoCreateLabel::Fetching => text.literal("ブランチ、タグ、Git オブジェクトをダウンロードしています。大きなリポジトリでは数分かかることがあります。"),
        RepoCreateLabel::Finalizing => text.literal("リポジトリの内容とデフォルトブランチを準備しています。"),
        RepoCreateLabel::FetchFailedRetry => text.literal("ミラーをフェッチできませんでした。リポジトリの管理者がミラーの設定から再試行できます。"),
        RepoCreateLabel::Complete => text.literal("ミラーが完了しました。リポジトリを開いています…"),
        RepoCreateLabel::Queued => text.literal("空いているミラーワーカーを待っています。"),
        RepoCreateLabel::Stopped => text.literal("ミラーが停止しました。リポジトリを開いています…"),
        RepoCreateLabel::Mirroring => text.literal("ミラー中"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("最短 ");
            text.number(minutes);
            text.literal(" 分です。");
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.argument(source);
            text.literal(" から：");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("ほか ");
            text.number(count);
            text.literal(" 件（サーバーログを参照してください）");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.argument(name);
            text.literal(" をミラー中");
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("ミラーをフェッチできませんでした：");
            text.argument(error);
        }
    }
}
