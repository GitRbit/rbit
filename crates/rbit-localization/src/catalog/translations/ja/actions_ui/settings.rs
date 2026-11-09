//! Japanese (`ja`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.argument(directory);
            text.literal(" 以下のワークフローは、");
            text.argument(repository);
            text.literal(" でイベントが発生するとランナー上で実行されます。");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.argument(organization);
            text.literal(
                " のリポジトリのワークフローで使うシークレットと変数です。リポジトリ自身に同じ名前のシークレットや変数がある場合は、そちらが優先されます。",
            );
        }
        ActionsSettingsMessage::AccountIntro => text.literal(
            "あなたのリポジトリのワークフローで使うシークレットと変数です。リポジトリ自身に同じ名前のシークレットや変数がある場合は、そちらが優先されます。",
        ),
        ActionsSettingsMessage::General => text.literal("全般"),
        ActionsSettingsMessage::RunWorkflows => text.literal("このリポジトリでワークフローを実行する"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal(
            "オフにすると、新しい実行は開始されず、Actions タブが非表示になります。過去の実行とそのログは保持されます。",
        ),
        ActionsSettingsMessage::DefaultPermission => text.literal("ワークフロートークンのデフォルト権限"),
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("ワークフローの ");
            text.argument(key);
            text.literal(
                " では権限を下げることしかできません。フォークや外部の作成者からの実行には、常に読み取り専用のトークンが渡されます。",
            );
        }
        ActionsSettingsMessage::PermissionRead => text.literal("読み取り"),
        ActionsSettingsMessage::PermissionReadHint => {
            text.literal("ワークフローはリポジトリの内容、Issue、プルリクエストを読み取れます。");
        }
        ActionsSettingsMessage::PermissionWrite => text.literal("読み取りと書き込み"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal(
            "ワークフローはプッシュや、Issue とプルリクエストへのコメントもできます。ブランチ保護は引き続き適用されます。",
        ),
        ActionsSettingsMessage::SaveChanges => text.literal("変更を保存"),
        ActionsSettingsMessage::RepositoryRunnersHint => {
            text.literal("このリポジトリのランナー、継承されたランナー、登録トークンです。");
        }
        ActionsSettingsMessage::OrganizationRunnersHint => {
            text.literal("組織のすべてのリポジトリ向けのランナーと登録トークンです。");
        }
        ActionsSettingsMessage::AccountRunnersHint => {
            text.literal("あなたのアカウントのすべてのリポジトリ向けのランナーと登録トークンです。");
        }
        ActionsSettingsMessage::EnvironmentsHint => text.literal(
            "デプロイ環境：必須レビュアー、許可するブランチ、環境独自のシークレットと変数を設定します。",
        ),
        ActionsSettingsMessage::TrustedTitle => text.literal("信頼済みのプルリクエスト作成者"),
        ActionsSettingsMessage::TrustedIntro => text.literal(
            "フォークからのプルリクエストは承認を待たずに実行されますが、引き続き読み取り専用のトークンで、シークレットや変数なしで実行されます。作成者は、実行の承認時に「この作成者のプルリクエストを常に承認なしで実行する」を選ぶと追加されます。",
        ),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("信頼済みの作成者はいません"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal(
            "フォークや書き込み権限のないユーザーからのプルリクエストは、実行ごとに書き込み権限を持つユーザーの承認を待ちます。",
        ),
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("信頼済み：");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("信頼済み：");
            text.argument(since);
            text.literal("（");
            text.argument(approver);
            text.literal(" が承認）");
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.argument(name);
            text.literal(" の信頼を解除");
        }
        ActionsSettingsMessage::Remove => text.literal("削除"),
        ActionsSettingsMessage::Saved => text.literal("設定を保存しました。"),
        ActionsSettingsMessage::SecretSaved => text.literal("シークレットを保存しました。"),
        ActionsSettingsMessage::SecretDeleted => text.literal("シークレットを削除しました。"),
        ActionsSettingsMessage::VariableSaved => text.literal("変数を保存しました。"),
        ActionsSettingsMessage::VariableDeleted => text.literal("変数を削除しました。"),
        ActionsSettingsMessage::Untrusted => text.literal("この作成者のプルリクエストには再び承認が必要になります。"),
        ActionsSettingsMessage::RepositoryAccess => text.literal("リポジトリのアクセス"),
        ActionsSettingsMessage::AllRepositories => text.literal("すべてのリポジトリ"),
        ActionsSettingsMessage::AllRepositoriesHint => {
            text.literal("このオーナーのすべてのリポジトリ（今後作成されるものを含む）。");
        }
        ActionsSettingsMessage::SelectedRepositories => text.literal("選択したリポジトリ"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("下でチェックしたリポジトリのみ。"),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("このオーナーにはまだリポジトリがありません。"),
    }
}
