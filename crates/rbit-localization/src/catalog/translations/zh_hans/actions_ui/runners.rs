//! Simplified Chinese (`zh-Hans`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;

/// "运行 {program} 并接收{owner or subject}作业的机器。"
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("运行 ");
    text.argument(program);
    text.literal(" 的机器，负责接收");
    match owner {
        Some(owner) => {
            text.literal(" ");
            text.argument(owner);
            text.literal(" 的");
        }
        None => text.literal(subject),
    }
    text.literal("作业。");
}

entries! { ZhHans:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("运行器"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal("实例运行器接收管理员允许的所有者的作业；设置 ");
            text.argument(setting);
            text.literal(" 后则接收所有所有者的作业。");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("注册令牌已创建"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal("请立即复制：它不会再次显示。在用于运行作业的机器上执行下面的命令，在其等待输入时粘贴令牌，然后用 ");
            text.argument(command);
            text.literal(" 启动运行器。");
        }
        ActionsRunnersMessage::Token => text.literal("令牌"),
        ActionsRunnersMessage::CopyToken => text.literal("复制令牌"),
        ActionsRunnersMessage::Copy => text.literal("复制"),
        ActionsRunnersMessage::Command => text.literal("命令"),
        ActionsRunnersMessage::CopyCommand => text.literal("复制命令"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("标签决定作业的运行方式：");
            text.argument(container);
            text.literal(" 在 Docker 或 Podman 中运行，");
            text.argument(host);
            text.literal(" 直接在机器上运行，没有任何隔离——仅适用于受信任的仓库。");
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "此仓库"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "你的仓库"),
        ActionsRunnersMessage::InstanceIntro { program } => intro(text, program, None, "所有获准使用实例运行器的所有者"),
        ActionsRunnersMessage::NoRunners => text.literal("还没有运行器"),
        ActionsRunnersMessage::NoRunnersHint => {
            text.literal("在下方创建注册令牌，并用它注册运行器。作业会一直等待，直到有匹配其标签的运行器上线。");
        }
        ActionsRunnersMessage::Online => text.literal("在线"),
        ActionsRunnersMessage::Offline => text.literal("离线"),
        ActionsRunnersMessage::Disabled => text.literal("已禁用"),
        ActionsRunnersMessage::NeverSeen => text.literal("从未上线"),
        ActionsRunnersMessage::OwnerScope => text.literal("所有者"),
        ActionsRunnersMessage::InstanceScope => text.literal("实例"),
        ActionsRunnersMessage::Ephemeral => text.literal("临时"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("主机——无隔离"),
        ActionsRunnersMessage::HostWarning => text.literal("使用其主机标签的作业将以运行器的用户身份运行，没有任何隔离"),
        ActionsRunnersMessage::Capacity { count } => {
            text.literal("同时 ");
            text.number(count);
            text.literal(" 个");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("上次在线 ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("启用"),
        ActionsRunnersMessage::Disable => text.literal("禁用"),
        ActionsRunnersMessage::Remove => text.literal("移除"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.literal("移除 ");
            text.argument(name);
        }
        ActionsRunnersMessage::TokensTitle => text.literal("注册令牌"),
        ActionsRunnersMessage::TokensIntro => {
            text.literal("运行器会用注册令牌一次性换取自己的凭据。除非另行选择，令牌只能使用一次，并会过期。");
        }
        ActionsRunnersMessage::ExpiresAfter => text.literal("有效期"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("仅限这些标签"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("（可选）"),
        ActionsRunnersMessage::Reusable => text.literal("在过期前允许任意数量的运行器用它注册"),
        ActionsRunnersMessage::NewToken => text.literal("新建注册令牌"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("创建于 ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("过期时间 ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("已不可用"),
        ActionsRunnersMessage::Revoked => text.literal("已吊销"),
        ActionsRunnersMessage::Expired => text.literal("已过期"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("不限次数"),
        ActionsRunnersMessage::UsedUp => text.literal("已用完"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.literal("剩余 ");
            text.number(count);
            text.literal(" 次");
        }
        ActionsRunnersMessage::Revoke => text.literal("吊销"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(" 小时");
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(" 天");
        }
    }
}
