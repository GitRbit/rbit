//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};

entries! { ZhHans:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("设置"),
        RepoSettingsLabel::AdminsOf => text.literal("只有"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("的管理员可以查看此页面。"),
        RepoSettingsLabel::SettingsSaved => text.literal("设置已保存。"),
        RepoSettingsLabel::General => text.literal("常规"),
        RepoSettingsLabel::Description => text.literal("描述"),
        RepoSettingsLabel::Optional => text.literal("可选"),
        RepoSettingsLabel::Visibility => text.literal("可见性"),
        RepoSettingsLabel::Public => text.literal("公开"),
        RepoSettingsLabel::PublicHint => text.literal("任何人都可以查看和克隆此仓库。"),
        RepoSettingsLabel::Private => text.literal("私有"),
        RepoSettingsLabel::PrivateHint => text.literal("只有拥有访问权限的人可以查看和克隆。"),
        RepoSettingsLabel::DefaultBranch => text.literal("默认分支"),
        RepoSettingsLabel::PushABranch => text.literal("推送一个分支后即可选择默认分支。"),
        RepoSettingsLabel::DefaultBranchHint => text.literal("浏览时首先显示，git clone 时默认检出。"),
        RepoSettingsLabel::SaveChanges => text.literal("保存更改"),
        RepoSettingsLabel::RepositoryName => text.literal("仓库名称"),
        RepoSettingsLabel::Name => text.literal("名称"),
        RepoSettingsLabel::RenameHint => text.literal("使用旧名称的链接和克隆 URL 将失效。"),
        RepoSettingsLabel::Rename => text.literal("重命名"),
        RepoSettingsLabel::DangerZone => text.literal("危险区域"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("取消归档此仓库"),
        RepoSettingsLabel::UnarchiveHint => {
            text.literal("仓库将重新接受推送、议题、拉取请求、评论和发行版，其镜像也会恢复。");
        }
        RepoSettingsLabel::Unarchive => text.literal("取消归档仓库"),
        RepoSettingsLabel::ArchiveTitle => text.literal("归档此仓库"),
        RepoSettingsLabel::ArchiveHint => text.literal(
            "仓库将变为只读：其他人仍可克隆、复刻它或为它加星标，但任何人都不能推送、合并或修改其议题、拉取请求或发行版，\
             其镜像也会暂停。你可以随时取消归档。",
        ),
        RepoSettingsLabel::Archive => text.literal("归档仓库"),
        RepoSettingsLabel::DeleteTitle => text.literal("删除此仓库"),
        RepoSettingsLabel::DeleteHint => text.literal("它的文件、历史记录和星标将被永久移除。此操作无法撤销。"),
        RepoSettingsLabel::Type => text.literal("输入"),
        RepoSettingsLabel::ToConfirm => text.literal("以确认"),
        RepoSettingsLabel::DeleteRepository => text.literal("删除仓库"),
        RepoSettingsLabel::DeployKeys => text.literal("部署密钥"),
        RepoSettingsLabel::DeployKeysHint => text.literal("仅授予服务器或 CI 作业通过 SSH 访问此仓库的密钥。"),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal("部署密钥仅授予服务器或 CI 作业通过 SSH 访问"),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal("的权限。除非允许推送，否则密钥是只读的。"),
        RepoSettingsLabel::NewDeployKey => text.literal("新建部署密钥"),
        RepoSettingsLabel::Title => text.literal("标题"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("例如：生产服务器"),
        RepoSettingsLabel::TitleHint => text.literal("默认使用密钥的注释。"),
        RepoSettingsLabel::Key => text.literal("密钥"),
        RepoSettingsLabel::KeyHint => text.literal("一个密钥只能属于一个仓库或账户；请为每个用途单独生成密钥。"),
        RepoSettingsLabel::AllowWrite => text.literal("允许写入"),
        RepoSettingsLabel::AllowWriteHint => text.literal("该密钥除了克隆，还可以推送到此仓库。"),
        RepoSettingsLabel::AddDeployKey => text.literal("添加部署密钥"),
        RepoSettingsLabel::Keys => text.literal("密钥"),
        RepoSettingsLabel::NoDeployKeys => text.literal("没有部署密钥"),
        RepoSettingsLabel::NoDeployKeysText => text.literal("添加一个密钥，让服务器或 CI 作业可以通过 SSH 克隆此仓库。"),
        RepoSettingsLabel::Delete => text.literal("删除"),
        RepoSettingsLabel::Mirror => text.literal("镜像"),
        RepoSettingsLabel::MirrorHint => text.literal("上游、获取间隔以及上次获取的结果。"),
        RepoSettingsLabel::MirrorLead => text.literal("此仓库从其上游获取分支和标记，不接受推送。"),
        RepoSettingsLabel::Upstream => text.literal("上游"),
        RepoSettingsLabel::LastFetch => text.literal("上次获取"),
        RepoSettingsLabel::Failed => text.literal("失败"),
        RepoSettingsLabel::Ok => text.literal("正常"),
        RepoSettingsLabel::NotYet => text.literal("尚未进行"),
        RepoSettingsLabel::NextFetch => text.literal("下次获取"),
        RepoSettingsLabel::Error => text.literal("错误"),
        RepoSettingsLabel::FetchNow => text.literal("立即获取"),
        RepoSettingsLabel::FetchEvery => text.literal("获取间隔"),
        RepoSettingsLabel::Minutes => text.literal("分钟"),
        RepoSettingsLabel::Save => text.literal("保存"),
        RepoSettingsLabel::Saved => text.literal("已保存。"),
        RepoSettingsLabel::FetchQueued => text.literal("获取已加入队列；稍后刷新即可查看结果。"),
        RepoSettingsLabel::PushQueued => text.literal("推送已加入队列；稍后刷新即可查看结果。"),
        RepoSettingsLabel::StopMirroring => text.literal("停止镜像"),
        RepoSettingsLabel::StopHint => text.literal("仓库会保留其内容，停止获取，并像其他仓库一样接受推送。"),
        RepoSettingsLabel::PushMirrors => text.literal("推送镜像"),
        RepoSettingsLabel::PushMirrorsHint => text.literal("每次推送后接收此仓库分支和标记的远程仓库。"),
        RepoSettingsLabel::PushMirrorsLead => text.literal(
            "接收此仓库分支和标记的远程仓库。每次推送都会覆盖远程仓库的分支和标记，并删除这里已不存在的分支和标记。",
        ),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("推送镜像已禁用"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal("此实例的管理员已关闭推送镜像。"),
        RepoSettingsLabel::NoPushMirrors => text.literal("还没有推送镜像"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal("在下方添加一个远程仓库，在其他位置保留此仓库的副本。"),
        RepoSettingsLabel::Paused => text.literal("已暂停"),
        RepoSettingsLabel::OnPush => text.literal("推送时同步"),
        RepoSettingsLabel::LastPush => text.literal("上次推送"),
        RepoSettingsLabel::NotPushedYet => text.literal("尚未推送"),
        RepoSettingsLabel::As => text.literal("用户"),
        RepoSettingsLabel::SecretConfigured => text.literal("已配置密码"),
        RepoSettingsLabel::PushNow => text.literal("立即推送"),
        RepoSettingsLabel::Edit => text.literal("编辑"),
        RepoSettingsLabel::Remove => text.literal("移除"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("添加推送镜像"),
        RepoSettingsLabel::AddPushMirror => text.literal("添加推送镜像"),
        RepoSettingsLabel::PushMirrorAdded => {
            text.literal("推送镜像已添加。首次推送正在进行；稍后刷新即可查看结果。");
        }
        RepoSettingsLabel::PushMirrorRemoved => text.literal("推送镜像已移除。远程仓库会保留现有内容。"),
        RepoSettingsLabel::EditPushMirror => text.literal("编辑推送镜像"),
        RepoSettingsLabel::PushMirror => text.literal("推送镜像"),
        RepoSettingsLabel::Enabled => text.literal("已启用"),
        RepoSettingsLabel::EnabledHint => text.literal("暂停的镜像会保留其设置，但不会被推送。"),
        RepoSettingsLabel::Cancel => text.literal("取消"),
        RepoSettingsLabel::RemoteUrl => text.literal("远程 URL"),
        RepoSettingsLabel::UrlHintStart => text.literal("不含用户名或密码的"),
        RepoSettingsLabel::UrlHintOr => text.literal("或"),
        RepoSettingsLabel::UrlHintEnd => text.literal("URL。"),
        RepoSettingsLabel::UserName => text.literal("用户名"),
        RepoSettingsLabel::PasswordOrToken => text.literal("密码或令牌"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("已配置——留空则保持不变"),
        RepoSettingsLabel::SecretHint => text.literal("通过 HTTP Basic 身份验证发送给远程仓库。保存后无法再次显示。"),
        RepoSettingsLabel::SecretHintKept => text.literal(
            "通过 HTTP Basic 身份验证发送给远程仓库。保存后无法再次显示；如果远程仓库迁移到其他主机，需要重新输入。",
        ),
        RepoSettingsLabel::PushAfterPush => text.literal("每次推送后同步推送"),
        RepoSettingsLabel::PushAfterPushHint => {
            text.literal("在此仓库发生变更几秒后推送。所有镜像也会按计划定期推送。");
        }
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("至少 ");
            text.number(minutes);
            text.literal(" 分钟。");
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("每个镜像至少每 ");
            text.number(minutes);
            text.literal(" 分钟推送一次。");
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.literal("删除 ");
            text.argument(title);
        }
    }
}
