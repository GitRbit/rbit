//! Japanese (`ja`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::TeamSentence;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};

entries! { Ja:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("新しいチーム"),
        TeamMessage::NoTeams => text.literal("チームはまだありません"),
        TeamMessage::NoTeamsText => text.literal("チームはメンバーをグループにまとめ、グループごとにリポジトリへのアクセス権を付与します。"),
        TeamMessage::OneMember => text.literal("メンバー 1 人"),
        TeamMessage::OneRepository => text.literal("リポジトリ 1 件"),
        TeamMessage::CreateTeam => text.literal("チームを作成"),
        TeamMessage::TeamSettings => text.literal("チームの設定"),
        TeamMessage::RenameHint => text.literal("名前を変更すると、チームのアドレスも変わります。"),
        TeamMessage::SaveChanges => text.literal("変更を保存"),
        TeamMessage::DeleteTeam => text.literal("このチームを削除"),
        TeamMessage::DeleteTeamText => text.literal("メンバーは組織に残りますが、このチームによって付与されていたアクセス権は失われます。"),
        TeamMessage::DeleteTeamButton => text.literal("チームを削除"),
        TeamMessage::UsernamePlaceholder => text.literal("ユーザー名"),
        TeamMessage::OnlyMembers => text.literal("組織のメンバーだけがチームに参加できます。"),
        TeamMessage::NoMembers => text.literal("メンバーはいません"),
        TeamMessage::NoMembersText => text.literal("メンバーには、このチームのリポジトリへのアクセス権が付与されます。"),
        TeamMessage::AddRepository => text.literal("リポジトリを追加"),
        TeamMessage::RepositoryPlaceholder => text.literal("リポジトリ名"),
        TeamMessage::Permission => text.literal("権限"),
        TeamMessage::RepositoryHint => text.literal("組織の任意のリポジトリを指定できます。チームのメンバーには、そのリポジトリに対するこの権限が付与されます。"),
        TeamMessage::NoRepositories => text.literal("リポジトリはありません"),
        TeamMessage::NoRepositoriesText => text.literal("このチームには、あなたが閲覧できるリポジトリへのアクセス権が付与されていません。"),
        TeamMessage::Private => text.literal("プライベート"),
        TeamMessage::Save => text.literal("保存"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.literal("メンバー ");
            text.number(count);
            text.literal(" 人");
        }
        TeamText::Repositories(count) => {
            text.literal("リポジトリ ");
            text.number(count);
            text.literal(" 件");
        }
        TeamText::PermissionOn(repository) => {
            text.argument(repository);
            text.literal(" に対する権限");
        }
        TeamText::SavePermissionOn(repository) => {
            text.argument(repository);
            text.literal(" に対する権限を保存");
        }
        TeamText::TeamsTitle(org) => {
            text.literal("チーム · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("新しいチーム · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("設定 · ");
            text.argument(team);
        }
    }
    TeamSentence => RichWriter |text| {
        TeamSentence::NameHint => {
            text.literal("英字、数字、");
            text.slot(0);
            text.literal("、");
            text.slot(1);
            text.literal(" および ");
            text.slot(2);
            text.literal(" が使用できます。チームのアドレスに使われます。");
        }
    }
}
