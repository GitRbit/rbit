//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/nav.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::RepoNavSentence;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};

entries! { ZhHans:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ArchivedNote => {
            text.literal("此仓库已归档，处于只读状态：你可以克隆、复刻它或为它加星标，但不能修改它。");
        }
        RepoNavLabel::Private => text.literal("私有"),
        RepoNavLabel::Public => text.literal("公开"),
        RepoNavLabel::Repository => text.literal("仓库"),
        RepoNavLabel::Code => text.literal("代码"),
        RepoNavLabel::Issues => text.literal("议题"),
        RepoNavLabel::PullRequests => text.literal("拉取请求"),
        RepoNavLabel::Commits => text.literal("提交"),
        RepoNavLabel::Branches => text.literal("分支"),
        RepoNavLabel::Tags => text.literal("标记"),
        RepoNavLabel::Releases => text.literal("发行版"),
        RepoNavLabel::Activity => text.literal("动态"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("设置"),
        RepoNavLabel::ForkThisRepository => text.literal("复刻此仓库"),
        RepoNavLabel::Fork => text.literal("复刻"),
        RepoNavLabel::Forks => text.literal("复刻"),
        RepoNavLabel::Clone => text.literal("克隆"),
        RepoNavLabel::CloneUrl => text.literal("克隆 URL"),
        RepoNavLabel::CopyCloneUrl => text.literal("复制克隆 URL"),
        RepoNavLabel::SshCloneUrl => text.literal("SSH 克隆 URL"),
        RepoNavLabel::CopySshCloneUrl => text.literal("复制 SSH 克隆 URL"),
        RepoNavLabel::Copy => text.literal("复制"),
        RepoNavLabel::YourSettings => text.literal("你的设置"),
        RepoNavLabel::DownloadZip => text.literal("下载 ZIP"),
        RepoNavLabel::Star => text.literal("加星标"),
        RepoNavLabel::Starred => text.literal("已加星标"),
        RepoNavLabel::StarThisRepository => text.literal("为此仓库加星标"),
        RepoNavLabel::UnstarThisRepository => text.literal("取消此仓库的星标"),
        RepoNavLabel::SignInToStar => text.literal("登录后即可为此仓库加星标"),
        RepoNavLabel::Stargazers => text.literal("星标用户"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(" 个复刻");
        }
        RepoNavMessage::StargazerCount { count } => {
            text.number(count.unsigned_abs());
            text.literal(" 位星标用户");
        }
    }
    RepoNavSentence => RichWriter |text| {
        RepoNavSentence::ForkedFrom => {
            text.literal("复刻自 ");
            text.slot(0);
        }
        RepoNavSentence::MirrorOf => {
            text.literal("镜像自 ");
            text.slot(0);
        }
        RepoNavSentence::SshKeyNote => {
            text.literal("使用 SSH 需要先添加密钥，请前往");
            text.slot(0);
            text.literal("。");
        }
    }
}
