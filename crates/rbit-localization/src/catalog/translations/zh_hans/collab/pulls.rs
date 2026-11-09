//! Simplified Chinese (`zh-Hans`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("拉取请求"),
        PullMessage::SearchPulls => text.literal("搜索拉取请求"),
        PullMessage::ClearSearch => text.literal("清除搜索和筛选"),
        PullMessage::FiltersLabel => text.literal("拉取请求筛选"),
        PullMessage::StateNav => text.literal("拉取请求状态"),
        PullMessage::AuthorFilter { author } => {
            text.literal("作者：");
            text.argument(author);
        }
        PullMessage::New => text.literal("新建"),
        PullMessage::NewSuffix => text.literal("拉取请求"),
        PullMessage::EmptyTitle => text.literal("这里没有拉取请求"),
        PullMessage::EmptyText => text.literal("拉取请求用于提议来自分支或复刻的更改。比较分支即可发起一个拉取请求。"),
        PullMessage::OpenTitle => text.literal("发起拉取请求"),
        PullMessage::OpenIntro => text.literal("将本仓库或复刻中某个分支的更改提议合并到基础分支。"),
        PullMessage::OpenAsDraft => text.literal("以草稿形式发起"),
        PullMessage::Create => text.literal("创建拉取请求"),
        PullMessage::DifferentHistories => text.literal("这些分支的历史记录完全不同。"),
        PullMessage::ChooseBranches => text.literal("选择两个现有分支进行比较。"),
        PullMessage::UpToDate { base, head } => {
            text.argument(base);
            text.literal(" 已包含 ");
            text.argument(head);
            text.literal(" 的所有提交。");
        }
        PullMessage::BaseRepository => text.literal("基础仓库"),
        PullMessage::BaseRefs => text.literal("基础分支和标记"),
        PullMessage::Base => text.literal("基础"),
        PullMessage::HeadRepository => text.literal("头部仓库"),
        PullMessage::Compare => text.literal("比较"),
        PullMessage::CompareRefs => text.literal("比较分支和标记"),
        PullMessage::PullRequest => text.literal("拉取请求"),
        PullMessage::Conversation => text.literal("对话"),
        PullMessage::Commits => text.literal("提交"),
        PullMessage::FilesChanged => text.literal("更改的文件"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.number(count);
            text.literal(" 个提交");
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("提交"),
        PullMessage::ChangeBase => text.literal("更改基础分支"),
        PullMessage::BaseBranch => text.literal("基础分支"),
        PullMessage::BaseBranches => text.literal("基础分支"),
        PullMessage::ChangeBaseHint => text.literal("更改基础分支会更新提交、差异和合并检查。"),
        PullMessage::UpdateBase => text.literal("更新基础分支"),
        PullMessage::IsClosed => text.literal("此拉取请求已关闭。"),
        PullMessage::IsMerged => text.literal("拉取请求已合并。"),
        PullMessage::DeleteBranch { branch } => {
            text.literal("删除 ");
            text.argument(branch);
        }
        PullMessage::ConvertToDraft => text.literal("转换为草稿"),
        PullMessage::NoCommitsTitle => text.literal("没有提交"),
        PullMessage::NoCommitsText => text.literal("头部分支和基础分支指向同一个提交。"),
        PullMessage::NewestCommits { count } => {
            text.literal("仅列出最新的 ");
            text.number(count);
            text.literal(" 个提交。");
        }
    }
}
