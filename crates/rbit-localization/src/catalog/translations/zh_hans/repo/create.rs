//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};

entries! { ZhHans:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("新建仓库"),
        RepoCreateLabel::RepositoryType => text.literal("仓库类型"),
        RepoCreateLabel::Empty => text.literal("空仓库"),
        RepoCreateLabel::Mirror => text.literal("镜像"),
        RepoCreateLabel::Import => text.literal("导入"),
        RepoCreateLabel::NewLead => text.literal("创建一个空仓库、导入一个项目，或者保持一个与源仓库同步的镜像。"),
        RepoCreateLabel::Owner => text.literal("所有者"),
        RepoCreateLabel::Name => text.literal("名称"),
        RepoCreateLabel::Description => text.literal("描述"),
        RepoCreateLabel::Optional => text.literal("可选"),
        RepoCreateLabel::Visibility => text.literal("可见性"),
        RepoCreateLabel::Public => text.literal("公开"),
        RepoCreateLabel::Private => text.literal("私有"),
        RepoCreateLabel::PublicRepository => text.literal("任何人都可以查看和克隆此仓库。"),
        RepoCreateLabel::PublicMirror => text.literal("任何人都可以查看和克隆此镜像。"),
        RepoCreateLabel::PublicImport => text.literal("任何人都可以查看和克隆该仓库。"),
        RepoCreateLabel::PrivateHint => text.literal("只有所有者可以查看。对于组织，即其成员。"),
        RepoCreateLabel::Cancel => text.literal("取消"),
        RepoCreateLabel::CreateRepository => text.literal("创建仓库"),
        RepoCreateLabel::MirrorLead => text.literal(
            "镜像会按计划从其他位置的公开仓库获取分支和标记。镜像在这里是只读的：更改来自上游。",
        ),
        RepoCreateLabel::MirrorsDisabled => text.literal("镜像已禁用"),
        RepoCreateLabel::MirrorsDisabledText => text.literal("此实例的管理员已关闭拉取镜像。"),
        RepoCreateLabel::UpstreamUrl => text.literal("上游 URL"),
        RepoCreateLabel::UrlHintStart => text.literal("公开仓库的"),
        RepoCreateLabel::UrlHintOr => text.literal("或"),
        RepoCreateLabel::UrlHintEnd => text.literal("URL。"),
        RepoCreateLabel::FromTheUrl => text.literal("取自 URL"),
        RepoCreateLabel::FetchEvery => text.literal("获取间隔"),
        RepoCreateLabel::Minutes => text.literal("分钟"),
        RepoCreateLabel::CreateMirror => text.literal("创建镜像"),
        RepoCreateLabel::MirrorARepository => text.literal("镜像仓库"),
        RepoCreateLabel::ImportLead => text.literal(
            "从 GitHub 或 Gitea 复制一个仓库及其历史记录，还可以按需复制其议题、拉取请求、标签、里程碑、发行版和 Git LFS 文件。\
             议题和拉取请求的编号保持不变。",
        ),
        RepoCreateLabel::ImportingDisabled => text.literal("导入已禁用"),
        RepoCreateLabel::ImportingDisabledText => text.literal("此实例的管理员已关闭导入功能。"),
        RepoCreateLabel::ImportFrom => text.literal("导入来源"),
        RepoCreateLabel::ServerAddress => text.literal("服务器地址"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("github.com 无需填写"),
        RepoCreateLabel::Repository => text.literal("仓库"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("owner/name 或其 URL"),
        RepoCreateLabel::AccessToken => text.literal("访问令牌"),
        RepoCreateLabel::OptionalForPublic => text.literal("公开仓库可不填"),
        RepoCreateLabel::TokenHint => text.literal(
            "可读取该仓库的令牌能提高来源的速率限制，导入私有仓库时必须提供。令牌仅在导入期间使用，绝不会被存储。",
        ),
        RepoCreateLabel::AsOnTheSource => text.literal("与来源相同"),
        RepoCreateLabel::BringAlong => text.literal("一并导入"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("不支持：此实例没有 Wiki。"),
        RepoCreateLabel::StartImport => text.literal("开始导入"),
        RepoCreateLabel::ImportARepository => text.literal("导入仓库"),
        RepoCreateLabel::ImportTitle => text.literal("导入"),
        RepoCreateLabel::FailedNote => text.literal("在此之前已导入的内容都已在仓库中。"),
        RepoCreateLabel::NotReached => text.literal("未进行"),
        RepoCreateLabel::Started => text.literal("开始时间"),
        RepoCreateLabel::WaitingForSlot => text.literal("正在等待空闲名额"),
        RepoCreateLabel::Ended => text.literal("结束时间"),
        RepoCreateLabel::UpdatesByItself => text.literal("此页面会自动更新。"),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("已跳过或不完整"),
        RepoCreateLabel::GoToRepository => text.literal("前往仓库"),
        RepoCreateLabel::OpensAutomatically => text.literal("首次镜像获取完成后，仓库会自动打开。"),
        RepoCreateLabel::FetchInProgress => text.literal("正在获取镜像"),
        RepoCreateLabel::TransferSizeNote => text.literal("传输大小取决于上游。此页面会随着进度显示当前阶段。"),
        RepoCreateLabel::RefreshStatus => text.literal("刷新状态"),
        RepoCreateLabel::SettingsAndRetry => text.literal("镜像设置和重试"),
        RepoCreateLabel::NoScriptHint => text.literal("使用“刷新状态”查看进度，并在仓库就绪后打开它。"),
        RepoCreateLabel::Connecting => text.literal("正在连接上游仓库。"),
        RepoCreateLabel::Fetching => text.literal("正在下载分支、标记和 Git 对象。大型仓库可能需要几分钟。"),
        RepoCreateLabel::Finalizing => text.literal("正在准备仓库内容和默认分支。"),
        RepoCreateLabel::FetchFailedRetry => text.literal("无法获取镜像。仓库管理员可以在镜像设置中重试。"),
        RepoCreateLabel::Complete => text.literal("镜像完成。正在打开仓库…"),
        RepoCreateLabel::Queued => text.literal("正在等待可用的镜像工作进程。"),
        RepoCreateLabel::Stopped => text.literal("镜像已停止。正在打开仓库…"),
        RepoCreateLabel::Mirroring => text.literal("正在镜像"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("至少 ");
            text.number(minutes);
            text.literal(" 分钟。");
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("来自 ");
            text.argument(source);
            text.literal("：");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("还有 ");
            text.number(count);
            text.literal(" 条（见服务器日志）");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.literal("正在镜像 ");
            text.argument(name);
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("无法获取镜像：");
            text.argument(error);
        }
    }
}
