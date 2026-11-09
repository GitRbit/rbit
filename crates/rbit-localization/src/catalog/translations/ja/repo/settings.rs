//! Japanese (`ja`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};

entries! { Ja:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("設定"),
        RepoSettingsLabel::AdminsOf => text.literal("このページは"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("の管理者のみが表示できます。"),
        RepoSettingsLabel::SettingsSaved => text.literal("設定を保存しました。"),
        RepoSettingsLabel::General => text.literal("全般"),
        RepoSettingsLabel::Description => text.literal("説明"),
        RepoSettingsLabel::Optional => text.literal("任意"),
        RepoSettingsLabel::Visibility => text.literal("公開範囲"),
        RepoSettingsLabel::Public => text.literal("パブリック"),
        RepoSettingsLabel::PublicHint => text.literal("誰でもこのリポジトリを表示、クローンできます。"),
        RepoSettingsLabel::Private => text.literal("プライベート"),
        RepoSettingsLabel::PrivateHint => text.literal("アクセス権を持つユーザーのみが表示、クローンできます。"),
        RepoSettingsLabel::DefaultBranch => text.literal("デフォルトブランチ"),
        RepoSettingsLabel::PushABranch => text.literal("デフォルトブランチを選択するには、ブランチをプッシュしてください。"),
        RepoSettingsLabel::DefaultBranchHint => text.literal("閲覧時に最初に表示され、git clone でチェックアウトされます。"),
        RepoSettingsLabel::SaveChanges => text.literal("変更を保存"),
        RepoSettingsLabel::RepositoryName => text.literal("リポジトリ名"),
        RepoSettingsLabel::Name => text.literal("名前"),
        RepoSettingsLabel::RenameHint => text.literal("古い名前を使ったリンクやクローン URL は使えなくなります。"),
        RepoSettingsLabel::Rename => text.literal("名前を変更"),
        RepoSettingsLabel::DangerZone => text.literal("危険な操作"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("このリポジトリのアーカイブを解除"),
        RepoSettingsLabel::UnarchiveHint => text.literal("プッシュ、Issue、プルリクエスト、コメント、リリースを再び受け付け、ミラーも再開されます。"),
        RepoSettingsLabel::Unarchive => text.literal("リポジトリのアーカイブを解除"),
        RepoSettingsLabel::ArchiveTitle => text.literal("このリポジトリをアーカイブ"),
        RepoSettingsLabel::ArchiveHint => text.literal("読み取り専用になります。クローン、フォーク、スターは引き続きできますが、誰もプッシュ、マージや Issue、プルリクエスト、リリースの変更はできなくなり、ミラーも一時停止します。アーカイブはいつでも解除できます。"),
        RepoSettingsLabel::Archive => text.literal("リポジトリをアーカイブ"),
        RepoSettingsLabel::DeleteTitle => text.literal("このリポジトリを削除"),
        RepoSettingsLabel::DeleteHint => text.literal("ファイル、履歴、スターは完全に削除されます。この操作は元に戻せません。"),
        RepoSettingsLabel::Type => text.literal("確認のため"),
        RepoSettingsLabel::ToConfirm => text.literal("と入力してください"),
        RepoSettingsLabel::DeleteRepository => text.literal("リポジトリを削除"),
        RepoSettingsLabel::DeployKeys => text.literal("デプロイキー"),
        RepoSettingsLabel::DeployKeysHint => text.literal("サーバーや CI ジョブに、このリポジトリだけへの SSH アクセスを許可するキーです。"),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal("デプロイキーは、サーバーや CI ジョブに"),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal("だけへの SSH アクセスを許可します。プッシュを許可しない限り、キーは読み取り専用です。"),
        RepoSettingsLabel::NewDeployKey => text.literal("新しいデプロイキー"),
        RepoSettingsLabel::Title => text.literal("タイトル"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("例：本番サーバー"),
        RepoSettingsLabel::TitleHint => text.literal("省略するとキーのコメントが使われます。"),
        RepoSettingsLabel::Key => text.literal("キー"),
        RepoSettingsLabel::KeyHint => text.literal("1 つのキーは 1 つのリポジトリまたはアカウントにしか登録できません。それぞれに別のキーを生成してください。"),
        RepoSettingsLabel::AllowWrite => text.literal("書き込みアクセスを許可"),
        RepoSettingsLabel::AllowWriteHint => text.literal("このキーでクローンに加えて、このリポジトリへのプッシュもできるようになります。"),
        RepoSettingsLabel::AddDeployKey => text.literal("デプロイキーを追加"),
        RepoSettingsLabel::Keys => text.literal("キー"),
        RepoSettingsLabel::NoDeployKeys => text.literal("デプロイキーはありません"),
        RepoSettingsLabel::NoDeployKeysText => text.literal("キーを追加すると、サーバーや CI ジョブが SSH でこのリポジトリをクローンできるようになります。"),
        RepoSettingsLabel::Delete => text.literal("削除"),
        RepoSettingsLabel::Mirror => text.literal("ミラー"),
        RepoSettingsLabel::MirrorHint => text.literal("アップストリーム、フェッチ間隔、前回のフェッチ結果。"),
        RepoSettingsLabel::MirrorLead => text.literal("このリポジトリはアップストリームのブランチとタグをフェッチし、プッシュは受け付けません。"),
        RepoSettingsLabel::Upstream => text.literal("アップストリーム"),
        RepoSettingsLabel::LastFetch => text.literal("前回のフェッチ"),
        RepoSettingsLabel::Failed => text.literal("失敗"),
        RepoSettingsLabel::Ok => text.literal("成功"),
        RepoSettingsLabel::NotYet => text.literal("未実行"),
        RepoSettingsLabel::NextFetch => text.literal("次回のフェッチ"),
        RepoSettingsLabel::Error => text.literal("エラー"),
        RepoSettingsLabel::FetchNow => text.literal("今すぐフェッチ"),
        RepoSettingsLabel::FetchEvery => text.literal("フェッチ間隔"),
        RepoSettingsLabel::Minutes => text.literal("分"),
        RepoSettingsLabel::Save => text.literal("保存"),
        RepoSettingsLabel::Saved => text.literal("保存しました。"),
        RepoSettingsLabel::FetchQueued => text.literal("フェッチをキューに追加しました。結果を確認するには、しばらくしてから再読み込みしてください。"),
        RepoSettingsLabel::PushQueued => text.literal("プッシュをキューに追加しました。結果を確認するには、しばらくしてから再読み込みしてください。"),
        RepoSettingsLabel::StopMirroring => text.literal("ミラーを停止"),
        RepoSettingsLabel::StopHint => text.literal("リポジトリの内容はそのまま残り、フェッチを停止して、通常のリポジトリと同様にプッシュを受け付けるようになります。"),
        RepoSettingsLabel::PushMirrors => text.literal("プッシュミラー"),
        RepoSettingsLabel::PushMirrorsHint => text.literal("プッシュのたびに、このリポジトリのブランチとタグを受け取るリモートです。"),
        RepoSettingsLabel::PushMirrorsLead => text.literal("このリポジトリのブランチとタグを受け取るリモートです。プッシュのたびにリモートのブランチとタグが上書きされ、ここに存在しなくなったものは削除されます。"),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("プッシュミラーは無効です"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal("このインスタンスの管理者がプッシュミラーを無効にしています。"),
        RepoSettingsLabel::NoPushMirrors => text.literal("プッシュミラーはまだありません"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal("下でリモートを追加すると、このリポジトリのコピーを別の場所に保持できます。"),
        RepoSettingsLabel::Paused => text.literal("一時停止中"),
        RepoSettingsLabel::OnPush => text.literal("プッシュ時"),
        RepoSettingsLabel::LastPush => text.literal("前回のプッシュ"),
        RepoSettingsLabel::NotPushedYet => text.literal("未プッシュ"),
        RepoSettingsLabel::As => text.literal("ユーザー："),
        RepoSettingsLabel::SecretConfigured => text.literal("シークレット設定済み"),
        RepoSettingsLabel::PushNow => text.literal("今すぐプッシュ"),
        RepoSettingsLabel::Edit => text.literal("編集"),
        RepoSettingsLabel::Remove => text.literal("削除"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("プッシュミラーを追加"),
        RepoSettingsLabel::AddPushMirror => text.literal("プッシュミラーを追加"),
        RepoSettingsLabel::PushMirrorAdded => text.literal("プッシュミラーを追加しました。最初のプッシュを実行中です。結果を確認するには、しばらくしてから再読み込みしてください。"),
        RepoSettingsLabel::PushMirrorRemoved => text.literal("プッシュミラーを削除しました。リモートの内容はそのまま残ります。"),
        RepoSettingsLabel::EditPushMirror => text.literal("プッシュミラーを編集"),
        RepoSettingsLabel::PushMirror => text.literal("プッシュミラー"),
        RepoSettingsLabel::Enabled => text.literal("有効"),
        RepoSettingsLabel::EnabledHint => text.literal("一時停止したミラーは設定を保持したまま、プッシュされません。"),
        RepoSettingsLabel::Cancel => text.literal("キャンセル"),
        RepoSettingsLabel::RemoteUrl => text.literal("リモート URL"),
        RepoSettingsLabel::UrlHintStart => text.literal(""),
        RepoSettingsLabel::UrlHintOr => text.literal("または"),
        RepoSettingsLabel::UrlHintEnd => text.literal("で始まり、ユーザー名やパスワードを含まない URL。"),
        RepoSettingsLabel::UserName => text.literal("ユーザー名"),
        RepoSettingsLabel::PasswordOrToken => text.literal("パスワードまたはトークン"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("設定済み：変更しない場合は空欄のまま"),
        RepoSettingsLabel::SecretHint => text.literal("HTTP Basic 認証でリモートに送信されます。再表示はできません。"),
        RepoSettingsLabel::SecretHintKept => text.literal("HTTP Basic 認証でリモートに送信されます。再表示はできず、リモートが別のホストに移った場合は再入力が必要です。"),
        RepoSettingsLabel::PushAfterPush => text.literal("プッシュのたびにプッシュ"),
        RepoSettingsLabel::PushAfterPushHint => text.literal("このリポジトリが変更されてから数秒後に実行されます。すべてのミラーは定期的にもプッシュされます。"),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("最短 ");
            text.number(minutes);
            text.literal(" 分です。");
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("すべてのミラーは少なくとも ");
            text.number(minutes);
            text.literal(" 分ごとにプッシュされます。");
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.argument(title);
            text.literal(" を削除");
        }
    }
}
