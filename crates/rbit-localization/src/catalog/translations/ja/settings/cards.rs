//! Japanese (`ja`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { Ja:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => text.literal("プッシュやプルリクエストでワークフローを実行します。ワークフローが使うシークレットと変数もここで管理します。"),
        CardMessage::OrgActions => text.literal("この組織のすべてのリポジトリのワークフローで使うシークレットと変数です。"),
        CardMessage::Collaborators => text.literal("コラボレーター"),
        CardMessage::CollaboratorsText => text.literal("このリポジトリにアクセスできるユーザーとチーム、およびそれぞれの権限です。"),
        CardMessage::Mirror => text.literal("ミラー"),
        CardMessage::MirrorText => text.literal("アップストリーム、フェッチの間隔、前回のフェッチの結果です。"),
        CardMessage::Protection => text.literal("ブランチ保護"),
        CardMessage::ProtectionText => text.literal("重要なブランチへの強制プッシュ、削除、プルリクエストを経ないプッシュを防ぎます。"),
        CardMessage::PushMirrors => text.literal("プッシュミラー"),
        CardMessage::PushMirrorsText => text.literal("プッシュのたびにこのリポジトリのブランチとタグを受け取るリモートです。"),
        CardMessage::DeployKeys => text.literal("デプロイキー"),
        CardMessage::DeployKeysText => text.literal("サーバーや CI ジョブにこのリポジトリだけへのアクセスを許可する SSH キーです。"),
        CardMessage::Teams => text.literal("チーム"),
        CardMessage::TeamsText => text.literal("メンバーをグループにまとめ、グループごとにリポジトリへのアクセス権を付与します。"),
        CardMessage::Transfer => text.literal("オーナーの移管"),
        CardMessage::TransferText => text.literal("このリポジトリを別のユーザーまたは組織に移します。URL が変わります。"),
        CardMessage::Webhooks => text.literal("Webhook"),
        CardMessage::RepoWebhooks => text.literal("コードのプッシュやリリースの公開などを他のサービスに通知します。"),
        CardMessage::OrgWebhooks => text.literal("この組織のすべてのリポジトリのイベントを他のサービスに通知します。"),
    }
}
