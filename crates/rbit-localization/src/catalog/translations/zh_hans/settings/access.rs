//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { ZhHans:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("设置"),
        AccessMessage::Collaborators => text.literal("协作者"),
        AccessMessage::IntroLead => text.literal("在此添加的人员将获得对"),
        AccessMessage::IntroDirectly => text.literal("的直接访问权限"),
        AccessMessage::IntroOrganization => text.literal("，无论他们是否属于该组织"),
        AccessMessage::IntroTail => text.literal("。"),
        AccessMessage::AddCollaborator => text.literal("添加协作者"),
        AccessMessage::Username => text.literal("用户名"),
        AccessMessage::Permission => text.literal("权限"),
        AccessMessage::Add => text.literal("添加"),
        AccessMessage::People => text.literal("人员"),
        AccessMessage::NoCollaborators => text.literal("没有协作者"),
        AccessMessage::NoCollaboratorsText => text.literal("按用户名添加某人，即可授予其对此仓库的访问权限。"),
        AccessMessage::You => text.literal("（你）"),
        AccessMessage::Save => text.literal("保存"),
        AccessMessage::Leave => text.literal("退出"),
        AccessMessage::Remove => text.literal("移除"),
        AccessMessage::Teams => text.literal("团队"),
        AccessMessage::ManageTeams => text.literal("管理团队"),
        AccessMessage::NoTeams => text.literal("没有团队"),
        AccessMessage::NoTeamsText => text.literal("在团队页面中为团队授予访问权限后，其成员就会在这里获得相应权限。"),
        AccessMessage::Transfer => text.literal("转移所有权"),
        AccessMessage::TransferLead => text.literal("将"),
        AccessMessage::TransferTail => text.literal(
            "转移到另一个账户。其历史记录、协作者和星标会一并转移；使用旧地址的链接和克隆 URL 将失效，\
             当前组织的团队也会失去访问权限。",
        ),
        AccessMessage::NewOwner => text.literal("新所有者"),
        AccessMessage::AdminEntry => text.literal("作为站点管理员，你可以输入任意用户或组织。"),
        AccessMessage::Organization => text.literal("组织"),
        AccessMessage::OwnerHint => text.literal("你的账户，或你所属的组织。"),
        AccessMessage::TypeLead => text.literal("输入"),
        AccessMessage::TypeTail => text.literal("以确认"),
        AccessMessage::TransferRepository => text.literal("转移仓库"),
        AccessMessage::Nowhere => text.literal("无处可转移"),
        AccessMessage::NowhereText => text.literal("你不属于任何可创建仓库的组织，而此仓库已经属于你。"),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.argument(name);
            text.literal(" 的权限");
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("保存 ");
            text.argument(name);
            text.literal(" 的权限");
        }
        AccessText::RemoveLabel(name) => {
            text.literal("移除 ");
            text.argument(name);
        }
        AccessText::Added(date) => {
            text.literal("添加于 ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("协作者 · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("转移 · ");
            text.argument(repository);
        }
    }
}
