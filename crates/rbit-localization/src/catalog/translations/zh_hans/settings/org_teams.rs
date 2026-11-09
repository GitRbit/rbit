//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::TeamSentence;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};

entries! { ZhHans:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("新建团队"),
        TeamMessage::NoTeams => text.literal("还没有团队"),
        TeamMessage::NoTeamsText => text.literal("团队将成员分组，并为每个组分别授予仓库访问权限。"),
        TeamMessage::OneMember => text.literal("1 位成员"),
        TeamMessage::OneRepository => text.literal("1 个仓库"),
        TeamMessage::CreateTeam => text.literal("创建团队"),
        TeamMessage::TeamSettings => text.literal("团队设置"),
        TeamMessage::RenameHint => text.literal("重命名会更改团队的地址。"),
        TeamMessage::SaveChanges => text.literal("保存更改"),
        TeamMessage::DeleteTeam => text.literal("删除此团队"),
        TeamMessage::DeleteTeamText => text.literal("其成员仍留在组织中，但会失去此团队授予的访问权限。"),
        TeamMessage::DeleteTeamButton => text.literal("删除团队"),
        TeamMessage::UsernamePlaceholder => text.literal("用户名"),
        TeamMessage::OnlyMembers => text.literal("只有组织成员才能加入其团队。"),
        TeamMessage::NoMembers => text.literal("没有成员"),
        TeamMessage::NoMembersText => text.literal("成员将获得此团队对其仓库的访问权限。"),
        TeamMessage::AddRepository => text.literal("添加仓库"),
        TeamMessage::RepositoryPlaceholder => text.literal("仓库名称"),
        TeamMessage::Permission => text.literal("权限"),
        TeamMessage::RepositoryHint => text.literal("组织的任意仓库。团队成员将获得对该仓库的此项权限。"),
        TeamMessage::NoRepositories => text.literal("没有仓库"),
        TeamMessage::NoRepositoriesText => text.literal("此团队尚未获得你可见的任何仓库的访问权限。"),
        TeamMessage::Private => text.literal("私有"),
        TeamMessage::Save => text.literal("保存"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(" 位成员");
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(" 个仓库");
        }
        TeamText::PermissionOn(repository) => {
            text.literal("对 ");
            text.argument(repository);
            text.literal(" 的权限");
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("保存对 ");
            text.argument(repository);
            text.literal(" 的权限");
        }
        TeamText::TeamsTitle(org) => {
            text.literal("团队 · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("新建团队 · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("设置 · ");
            text.argument(team);
        }
    }
    TeamSentence => RichWriter |text| {
        TeamSentence::NameHint => {
            text.literal("可使用字母、数字、");
            text.slot(0);
            text.literal("、");
            text.slot(1);
            text.literal(" 和 ");
            text.slot(2);
            text.literal("；用于团队的地址。");
        }
    }
}
