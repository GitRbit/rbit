//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { ZhHans:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("请选择一个权限。"),
        OrgMessage::Organization => text.literal("组织"),
        OrgMessage::Repositories => text.literal("仓库"),
        OrgMessage::Members => text.literal("成员"),
        OrgMessage::Teams => text.literal("团队"),
        OrgMessage::Settings => text.literal("设置"),
        OrgMessage::NewOrganization => text.literal("新建组织"),
        OrgMessage::NewIntro => text.literal("组织代表团队拥有仓库。你将成为它的第一位所有者。"),
        OrgMessage::Name => text.literal("名称"),
        OrgMessage::NameHint => {
            text.literal("用于 URL。可使用字母、数字、“-”、“_”和“.”。用户和组织共用同一个命名空间。");
        }
        OrgMessage::DisplayName => text.literal("显示名称"),
        OrgMessage::Optional => text.literal("可选"),
        OrgMessage::Description => text.literal("描述"),
        OrgMessage::Cancel => text.literal("取消"),
        OrgMessage::CreateOrganization => text.literal("创建组织"),
        OrgMessage::New => text.literal("新建"),
        OrgMessage::NoRepositories => text.literal("还没有仓库"),
        OrgMessage::NoRepositoriesText => text.literal("此组织拥有的仓库会列在这里。"),
        OrgMessage::Profile => text.literal("资料"),
        OrgMessage::Website => text.literal("网站"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("保存资料"),
        OrgMessage::MemberPermission => text.literal("成员权限"),
        OrgMessage::PermissionLegend => text.literal("每位成员对每个仓库拥有的权限"),
        OrgMessage::PermissionHint => {
            text.literal("团队和协作者只能在此基础上增加权限，不能减少。所有者始终可以管理每个仓库。");
        }
        OrgMessage::SavePermission => text.literal("保存权限"),
        OrgMessage::NoPermission => text.literal("无权限"),
        OrgMessage::NoPermissionText => text.literal("成员只能看到公开仓库，以及其团队可以访问的仓库。"),
        OrgMessage::Read => text.literal("读取"),
        OrgMessage::ReadText => text.literal("成员可以浏览和克隆每个仓库。"),
        OrgMessage::Write => text.literal("写入"),
        OrgMessage::WriteText => text.literal("成员可以浏览、克隆和推送到每个仓库。"),
        OrgMessage::DangerZone => text.literal("危险区域"),
        OrgMessage::DeleteOrganization => text.literal("删除此组织"),
        OrgMessage::DeleteText => text.literal("这将永久移除该组织及其所有成员关系。此操作无法撤销。"),
        OrgMessage::TypeToConfirmLead => text.literal("输入"),
        OrgMessage::TypeToConfirmTail => text.literal("以确认"),
        OrgMessage::DeleteOrganizationButton => text.literal("删除组织"),
        OrgMessage::AddMember => text.literal("添加成员"),
        OrgMessage::Username => text.literal("用户名"),
        OrgMessage::Role => text.literal("角色"),
        OrgMessage::Member => text.literal("成员"),
        OrgMessage::Owner => text.literal("所有者"),
        OrgMessage::Add => text.literal("添加"),
        OrgMessage::MembersHint => text.literal(
            "成员在每个仓库上都拥有组织的基础权限，外加其团队授予的权限。所有者还可以管理成员、团队和设置。",
        ),
        OrgMessage::You => text.literal("（你）"),
        OrgMessage::Leave => text.literal("退出"),
        OrgMessage::Remove => text.literal("移除"),
        OrgMessage::MakeOwner => text.literal("设为所有者"),
        OrgMessage::MakeMember => text.literal("设为成员"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" 仍拥有 1 个仓库。请先删除或转移它。");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" 仍拥有 ");
            text.number(count);
            text.literal(" 个仓库。请先删除或转移它们。");
        }
        OrgText::RemoveLabel(name) => {
            text.literal("移除 ");
            text.argument(name);
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal("：");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("成员 · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("设置 · ");
            text.argument(org);
        }
    }
}
