//! Japanese (`ja`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;

/// 「{program} を実行し、{subject} のジョブを受け取るマシンです。」
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.argument(program);
    text.literal(" を実行し、");
    match owner {
        Some(owner) => {
            text.argument(owner);
            text.literal(" ");
        }
        None => text.literal(subject),
    }
    text.literal("のジョブを受け取るマシンです。");
}

entries! { Ja:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("ランナー"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal(
                "インスタンスランナーは、管理者が許可したオーナーのジョブを受け取ります。すべてのオーナーに開放するには ",
            );
            text.argument(setting);
            text.literal(" を使用します。");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("登録トークンを作成しました"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal(
                "今すぐコピーしてください。再表示はされません。ジョブを実行するマシンで下のコマンドを実行し、入力を求められたらトークンを貼り付けてから、",
            );
            text.argument(command);
            text.literal(" でランナーを起動します。");
        }
        ActionsRunnersMessage::Token => text.literal("トークン"),
        ActionsRunnersMessage::CopyToken => text.literal("トークンをコピー"),
        ActionsRunnersMessage::Copy => text.literal("コピー"),
        ActionsRunnersMessage::Command => text.literal("コマンド"),
        ActionsRunnersMessage::CopyCommand => text.literal("コマンドをコピー"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("ラベルはジョブの実行方法を表します。");
            text.argument(container);
            text.literal(" は Docker または Podman で実行し、");
            text.argument(host);
            text.literal(" は分離なしでマシン上で直接実行します（信頼できるリポジトリ専用）。");
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "このリポジトリ"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "あなたのリポジトリ"),
        ActionsRunnersMessage::InstanceIntro { program } => {
            intro(text, program, None, "インスタンスランナーの使用を許可されたすべてのオーナー");
        }
        ActionsRunnersMessage::NoRunners => text.literal("ランナーはまだありません"),
        ActionsRunnersMessage::NoRunnersHint => text.literal(
            "下で登録トークンを作成し、それを使ってランナーを登録してください。ジョブは、そのラベルを持つランナーがオンラインになるまで待機します。",
        ),
        ActionsRunnersMessage::Online => text.literal("オンライン"),
        ActionsRunnersMessage::Offline => text.literal("オフライン"),
        ActionsRunnersMessage::Disabled => text.literal("無効"),
        ActionsRunnersMessage::NeverSeen => text.literal("未接続"),
        ActionsRunnersMessage::OwnerScope => text.literal("オーナー"),
        ActionsRunnersMessage::InstanceScope => text.literal("インスタンス"),
        ActionsRunnersMessage::Ephemeral => text.literal("使い捨て"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("ホスト — 分離なし"),
        ActionsRunnersMessage::HostWarning => {
            text.literal("ホストラベルのジョブは、分離なしでランナーのユーザーとして実行されます");
        }
        ActionsRunnersMessage::Capacity { count } => {
            text.literal("同時に ");
            text.number(count);
            text.literal(" 件");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("最終確認：");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("有効にする"),
        ActionsRunnersMessage::Disable => text.literal("無効にする"),
        ActionsRunnersMessage::Remove => text.literal("削除"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.argument(name);
            text.literal(" を削除");
        }
        ActionsRunnersMessage::TokensTitle => text.literal("登録トークン"),
        ActionsRunnersMessage::TokensIntro => text.literal(
            "ランナーは登録トークンを一度だけ自身の認証情報と交換します。トークンは、別の設定を選ばない限り、1 回限りの使用で有効期限があります。",
        ),
        ActionsRunnersMessage::ExpiresAfter => text.literal("有効期間"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("これらのラベルのみ"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("（任意）"),
        ActionsRunnersMessage::Reusable => {
            text.literal("有効期限まで、何台のランナーでもこのトークンで登録できるようにする");
        }
        ActionsRunnersMessage::NewToken => text.literal("新しい登録トークン"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("作成：");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("有効期限：");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("使用不可"),
        ActionsRunnersMessage::Revoked => text.literal("取り消し済み"),
        ActionsRunnersMessage::Expired => text.literal("期限切れ"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("使用回数無制限"),
        ActionsRunnersMessage::UsedUp => text.literal("使用済み"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.literal("残り ");
            text.number(count);
            text.literal(" 回");
        }
        ActionsRunnersMessage::Revoke => text.literal("取り消す"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(" 時間");
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(" 日");
        }
    }
}
