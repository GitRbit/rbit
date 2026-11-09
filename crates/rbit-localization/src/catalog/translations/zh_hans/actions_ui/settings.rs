//! Simplified Chinese (`zh-Hans`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.argument(directory);
            text.literal(" 下的工作流会在 ");
            text.argument(repository);
            text.literal(" 中发生相应事件时，在运行器上运行。");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("供 ");
            text.argument(organization);
            text.literal(" 的仓库中的工作流使用的机密和变量。仓库自己的同名机密或变量优先。");
        }
        ActionsSettingsMessage::AccountIntro => {
            text.literal("供你自己的仓库中的工作流使用的机密和变量。仓库自己的同名机密或变量优先。");
        }
        ActionsSettingsMessage::General => text.literal("常规"),
        ActionsSettingsMessage::RunWorkflows => text.literal("在此仓库中运行工作流"),
        ActionsSettingsMessage::RunWorkflowsHint => {
            text.literal("关闭后：不会启动新的运行，并隐藏 Actions 标签页。之前的运行及其日志会保留。");
        }
        ActionsSettingsMessage::DefaultPermission => text.literal("工作流令牌的默认权限"),
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("工作流的 ");
            text.argument(key);
            text.literal(" 只能降低该权限。来自复刻和外部作者的运行始终只获得只读令牌。");
        }
        ActionsSettingsMessage::PermissionRead => text.literal("读取"),
        ActionsSettingsMessage::PermissionReadHint => text.literal("工作流可以读取仓库的内容、议题和拉取请求。"),
        ActionsSettingsMessage::PermissionWrite => text.literal("读取和写入"),
        ActionsSettingsMessage::PermissionWriteHint => {
            text.literal("工作流还可以推送，以及在议题和拉取请求中发表评论。分支保护仍然有效。");
        }
        ActionsSettingsMessage::SaveChanges => text.literal("保存更改"),
        ActionsSettingsMessage::RepositoryRunnersHint => text.literal("此仓库的运行器、它继承的运行器，以及注册令牌。"),
        ActionsSettingsMessage::OrganizationRunnersHint => text.literal("供组织所有仓库使用的运行器，以及注册令牌。"),
        ActionsSettingsMessage::AccountRunnersHint => text.literal("供你账户所有仓库使用的运行器，以及注册令牌。"),
        ActionsSettingsMessage::EnvironmentsHint => {
            text.literal("部署环境：必需的审查者、允许的分支，以及环境自己的机密和变量。");
        }
        ActionsSettingsMessage::TrustedTitle => text.literal("受信任的拉取请求作者"),
        ActionsSettingsMessage::TrustedIntro => text.literal(
            "他们来自复刻的拉取请求无需等待批准即可运行，但仍只使用只读令牌，且没有机密或变量。\
             在批准运行时勾选“始终运行此作者的拉取请求”即可添加作者。",
        ),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("没有受信任的作者"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => {
            text.literal("来自复刻和没有写入权限的用户的拉取请求，每次运行都需要等待有写入权限的成员批准。");
        }
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("信任于 ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("由 ");
            text.argument(approver);
            text.literal(" 信任于 ");
            text.argument(since);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.literal("不再信任 ");
            text.argument(name);
        }
        ActionsSettingsMessage::Remove => text.literal("移除"),
        ActionsSettingsMessage::Saved => text.literal("设置已保存。"),
        ActionsSettingsMessage::SecretSaved => text.literal("机密已保存。"),
        ActionsSettingsMessage::SecretDeleted => text.literal("机密已删除。"),
        ActionsSettingsMessage::VariableSaved => text.literal("变量已保存。"),
        ActionsSettingsMessage::VariableDeleted => text.literal("变量已删除。"),
        ActionsSettingsMessage::Untrusted => text.literal("该作者的拉取请求重新需要批准。"),
        ActionsSettingsMessage::RepositoryAccess => text.literal("仓库访问"),
        ActionsSettingsMessage::AllRepositories => text.literal("所有仓库"),
        ActionsSettingsMessage::AllRepositoriesHint => text.literal("此所有者的所有仓库，包括以后创建的仓库。"),
        ActionsSettingsMessage::SelectedRepositories => text.literal("选定的仓库"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("仅限下方勾选的仓库。"),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("此所有者还没有仓库。"),
    }
}
