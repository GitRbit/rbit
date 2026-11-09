//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/cards.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::cards::CardMessage;

entries! { ZhHans:
    CardMessage => MessageWriter |text| {
        CardMessage::Actions => text.literal("Actions"),
        CardMessage::RepoActions => text.literal("在推送和拉取请求时运行工作流，并管理它们使用的机密和变量。"),
        CardMessage::OrgActions => text.literal("供此组织所有仓库的工作流使用的机密和变量。"),
        CardMessage::Collaborators => text.literal("协作者"),
        CardMessage::CollaboratorsText => text.literal("有权访问此仓库的人员和团队，以及各自可以执行的操作。"),
        CardMessage::Mirror => text.literal("镜像"),
        CardMessage::MirrorText => text.literal("上游、获取间隔以及上次获取的结果。"),
        CardMessage::Protection => text.literal("分支保护"),
        CardMessage::ProtectionText => text.literal("防止重要分支被强制推送、删除，或在没有拉取请求的情况下被推送。"),
        CardMessage::PushMirrors => text.literal("推送镜像"),
        CardMessage::PushMirrorsText => text.literal("每次推送后接收此仓库分支和标记的远程仓库。"),
        CardMessage::DeployKeys => text.literal("部署密钥"),
        CardMessage::DeployKeysText => text.literal("仅授予服务器或 CI 作业访问此仓库权限的 SSH 密钥。"),
        CardMessage::Teams => text.literal("团队"),
        CardMessage::TeamsText => text.literal("将成员分组，并为每个组分别授予仓库访问权限。"),
        CardMessage::Transfer => text.literal("转移所有权"),
        CardMessage::TransferText => text.literal("将此仓库转移给其他用户或组织。其 URL 会随之改变。"),
        CardMessage::Webhooks => text.literal("Webhook"),
        CardMessage::RepoWebhooks => text.literal("在推送代码、发布发行版等事件发生时通知其他服务。"),
        CardMessage::OrgWebhooks => text.literal("在此组织的任一仓库发生事件时通知其他服务。"),
    }
}
