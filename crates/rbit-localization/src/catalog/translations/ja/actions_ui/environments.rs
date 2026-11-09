//! Japanese (`ja`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("環境"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.argument(key);
            text.literal(
                " で環境を指定したジョブは、その環境のシークレットと変数を受け取り、レビュアーがいればそのいずれかの承認を待ち、許可されたブランチからのみ実行されます。ワークフローが初めて環境を指定すると、ここに表示されます。",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("環境はありません"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("下で追加するか、ジョブが ");
            text.argument(key);
            text.literal(" で環境を指定しているワークフローを実行してください。");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("新しい環境"),
        ActionsEnvironmentMessage::Name => text.literal("名前"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("環境を追加"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.literal("レビュアー ");
            text.number(count);
            text.literal(" 人");
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("保護ブランチ"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("ブランチ ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("保護なし"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("保護ルール"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("必須レビュアー"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("1 行に 1 つのユーザー名"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal(
            "このリポジトリを読み取れるユーザーを 6 人まで指定できます。この環境のジョブは、そのいずれかが承認するまで待機し、却下されたジョブは失敗します。指定しない場合、ジョブはレビューなしで実行されます。",
        ),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("セルフレビューを禁止"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => {
            text.literal("実行を開始したユーザーは、そのデプロイを承認できません。");
        }
        ActionsEnvironmentMessage::WaitTimer => text.literal("待機タイマー"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal(
            "この環境のジョブが準備でき承認された後、実行されるまで待機する分数です。待機しない場合は 0、最大 43200（30 日）です。",
        ),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("デプロイ可能なブランチとタグ"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("すべてのブランチとタグ"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("保護ブランチのみ"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal("ブランチ保護ルールが設定されたブランチです。"),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("選択したブランチとタグ"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("ブランチとタグのパターン"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("ワークフローのフィルターと同じく、1 行に 1 つのパターンを指定します。ブランチは名前で（");
            text.argument(branch);
            text.literal("）、タグは ");
            text.argument(tags);
            text.literal(" で（");
            text.argument(tag);
            text.literal("）指定します。");
            text.argument(exclude);
            text.literal(" で除外します。");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("保護ルールを保存"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("この環境を削除"),
        ActionsEnvironmentMessage::DeleteHint => text.literal(
            "環境のシークレット、変数、ルールも削除されます。この環境のレビュアーを待っているジョブはすぐに実行されます。",
        ),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("環境を削除"),
    }
}
