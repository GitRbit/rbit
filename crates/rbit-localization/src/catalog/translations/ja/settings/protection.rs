//! Japanese (`ja`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};

entries! { Ja:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("確認のためチェックボックスをオンにしてください。"),
        ProtectionMessage::Settings => text.literal("設定"),
        ProtectionMessage::Title => text.literal("ブランチ保護"),
        ProtectionMessage::IntroLead => text.literal("ルールは"),
        ProtectionMessage::IntroTail => text.literal("へのすべてのプッシュに、HTTP と SSH のどちらでも適用されます。ルールでバイパスが許可されていない限り、管理者も対象です。"),
        ProtectionMessage::AddRule => text.literal("ルールを追加"),
        ProtectionMessage::Deleted => text.literal("ルールを削除しました。"),
        ProtectionMessage::Empty => text.literal("保護ブランチはありません"),
        ProtectionMessage::EmptyText => text.literal("ルールを追加すると、main などのブランチへのフォースプッシュ、削除、プルリクエストを経ない変更を防げます。"),
        ProtectionMessage::NoMatchYet => text.literal("まだどのブランチにも一致しません。"),
        ProtectionMessage::MatchesLabel => text.literal("一致："),
        ProtectionMessage::NewRule => text.literal("新しいブランチ保護ルール"),
        ProtectionMessage::NewRuleIntro => text.literal("ルールは、パターンに一致するすべてのブランチに、現在も今後も適用されます。"),
        ProtectionMessage::Saved => text.literal("ルールを保存しました。"),
        ProtectionMessage::Rule => text.literal("ルール"),
        ProtectionMessage::Pattern => text.literal("ブランチ名のパターン"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("ブランチ名またはパターンを指定します："),
        ProtectionMessage::PatternOne => text.literal("は名前の 1 つの区切り内で一致、"),
        ProtectionMessage::PatternAcross => text.literal("は区切りをまたいで一致（"),
        ProtectionMessage::PatternChar => text.literal("）、"),
        ProtectionMessage::PatternTail => text.literal("は任意の 1 文字に一致します。大文字と小文字は区別されます。"),
        ProtectionMessage::Protect => text.literal("一致するブランチを保護"),
        ProtectionMessage::BlockForcePush => text.literal("フォースプッシュを禁止"),
        ProtectionMessage::BlockForcePushText => text.literal("プッシュではコミットの追加のみ可能で、履歴は書き換えられません。"),
        ProtectionMessage::BlockDeletion => text.literal("削除を禁止"),
        ProtectionMessage::BlockDeletionText => text.literal("一致するブランチは削除できません。"),
        ProtectionMessage::RequirePullRequest => text.literal("プルリクエストを必須にする"),
        ProtectionMessage::RequirePullRequestText => text.literal("変更はプルリクエストのマージによってのみ取り込まれます。既存のブランチへの直接のプッシュは拒否されます。"),
        ProtectionMessage::RequiredApprovals => text.literal("必要な承認数"),
        ProtectionMessage::DismissStale => text.literal("新しいコミットがプッシュされたら承認を取り消す"),
        ProtectionMessage::BlockOnChanges => text.literal("変更がリクエストされている間はマージを禁止"),
        ProtectionMessage::RestrictPushers => text.literal("プッシュできるユーザーを制限"),
        ProtectionMessage::RestrictPeople => text.literal("以下のユーザーだけがプッシュまたはマージできます。デプロイキーは常に不可です。"),
        ProtectionMessage::RestrictPeopleTeams => text.literal("以下のユーザーとチームだけがプッシュまたはマージできます。デプロイキーは常に不可です。"),
        ProtectionMessage::People => text.literal("ユーザー"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal("ユーザー名をスペースまたはカンマで区切って入力します。各ユーザーには書き込み権限が必要です。"),
        ProtectionMessage::Teams => text.literal("チーム"),
        ProtectionMessage::NoTeams => text.literal("この組織にはまだチームがありません。"),
        ProtectionMessage::RequireChecks => text.literal("ステータスチェックの成功を必須にする"),
        ProtectionMessage::RequireChecksText => text.literal("ヘッドコミットで以下のすべてのチェックの最新ステータスが成功の場合にのみ、プルリクエストをマージできます。"),
        ProtectionMessage::RequiredChecks => text.literal("必須チェック"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("管理者のバイパスを許可"),
        ProtectionMessage::AdminBypassText => text.literal("リポジトリの管理者にはこのルールが適用されません。バイパスはすべて監査ログに記録されます。"),
        ProtectionMessage::SaveRule => text.literal("ルールを保存"),
        ProtectionMessage::DangerZone => text.literal("危険な操作"),
        ProtectionMessage::DeleteTitle => text.literal("このルールを削除"),
        ProtectionMessage::DeleteLead => text.literal("パターン"),
        ProtectionMessage::DeleteTail => text.literal("に一致するブランチは、ただちに保護されなくなります。"),
        ProtectionMessage::DeleteConfirm => text.literal("このルールが削除されることを理解しました"),
        ProtectionMessage::DeleteRule => text.literal("ルールを削除"),
        ProtectionMessage::NoMatches => text.literal("まだどのブランチにも一致しません"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("このリポジトリのルールは上限の ");
            text.number(max);
            text.literal(" 件に達しています。追加するには、いずれかを削除してください。");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal("1 行に 1 つのコンテキストを、CI が報告するとおりに（大文字と小文字を区別して）入力します。最大 ");
            text.number(max);
            text.literal(" 件です。");
        }
        ProtectionText::Matches(names) => {
            text.literal("一致：");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("一致：");
            text.argument(names);
            text.literal(" ほか ");
            text.number(more);
            text.literal(" 件");
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("ブランチ保護 · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("新しいブランチ保護ルール · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · ブランチ保護 · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("フォースプッシュ禁止"),
        RuleBadge::NoDeletion => text.literal("削除禁止"),
        RuleBadge::PullRequest => text.literal("プルリクエスト"),
        RuleBadge::Approvals(count) => {
            text.literal("承認 ");
            text.number(count);
            text.literal(" 件");
        }
        RuleBadge::StatusChecks => text.literal("ステータスチェック"),
        RuleBadge::Restricted => text.literal("制限あり"),
        RuleBadge::AdminsBypass => text.literal("管理者はバイパス可"),
    }
}
