//! Japanese (`ja`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("シークレット"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("ワークフローからは ");
            text.argument(expression);
            text.literal(
                " として参照します。値は暗号化され、再表示はできず、置き換えのみ可能です。フォークや書き込み権限のない作成者からの実行には渡されません。プッシュできるユーザーは誰でもワークフローを通じて読み取れます。",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("シークレットを保存または使用できません：");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("シークレットはありません"),
        ActionsSecretsMessage::NoSecretsHint => {
            text.literal("ワークフローに必要なトークンや鍵をシークレットとして追加してください。");
        }
        ActionsSecretsMessage::CannotDecrypt => text.literal("復号できません — もう一度設定してください"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("更新：");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("すべてのリポジトリ"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(" 個の選択したリポジトリ");
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("シークレット ");
            text.argument(name);
            text.literal(" を削除");
        }
        ActionsSecretsMessage::Delete => text.literal("削除"),
        ActionsSecretsMessage::ReplaceValue => text.literal("値を置き換え"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.argument(name);
            text.literal(" の新しい値");
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("シークレットを置き換え"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("リポジトリを選択"),
        ActionsSecretsMessage::Recipients { name } => {
            text.argument(name);
            text.literal(" を受け取るワークフローのリポジトリです。値は変更されません。");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("リポジトリを保存"),
        ActionsSecretsMessage::NewSecret => text.literal("新しいシークレット"),
        ActionsSecretsMessage::Name => text.literal("名前"),
        ActionsSecretsMessage::SecretNameHint => text.literal(
            "英字、数字、アンダースコアを使用できます。数字、または GITHUB_、GITEA_、FORGEJO_、RBIT_ で始めることはできません。名前は大文字で保存され、既存の名前は置き換えられます。",
        ),
        ActionsSecretsMessage::Value => text.literal("値"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("最大 ");
            text.number(kib);
            text.literal(" KiB です。再表示はできません。");
        }
        ActionsSecretsMessage::AddSecret => text.literal("シークレットを追加"),
        ActionsSecretsMessage::VariablesTitle => text.literal("変数"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("ワークフローからは ");
            text.argument(expression);
            text.literal(
                " として参照します。変数は秘密ではなく、ワークフローを変更できるユーザーは誰でも出力できます。フォークや書き込み権限のない作成者からの実行には渡されません。",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("変数はありません"),
        ActionsSecretsMessage::NoVariablesHint => {
            text.literal("リージョンやバージョンなど、ワークフロー間で共有する構成を変数として追加してください。");
        }
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("変数 ");
            text.argument(name);
            text.literal(" を削除");
        }
        ActionsSecretsMessage::Edit => text.literal("編集"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.argument(name);
            text.literal(" の値");
        }
        ActionsSecretsMessage::SaveVariable => text.literal("変数を保存"),
        ActionsSecretsMessage::NewVariable => text.literal("新しい変数"),
        ActionsSecretsMessage::VariableNameHint => {
            text.literal("シークレット名と同じ規則です。既存の名前は置き換えられます。");
        }
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("最大 ");
            text.number(kib);
            text.literal(" KiB です。");
        }
        ActionsSecretsMessage::AddVariable => text.literal("変数を追加"),
    }
}
