//! Simplified Chinese (`zh-Hans`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("环境"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("用 ");
            text.argument(key);
            text.literal(
                " 指定了环境的作业会获得该环境的机密和变量；若环境设有审查者，作业会等待其中一位审查者批准；\
                 并且只能从环境允许的分支运行。工作流第一次指定某个环境时，该环境就会出现在这里。",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("没有环境"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("在下方添加一个环境，或运行一个其作业用 ");
            text.argument(key);
            text.literal(" 指定了环境的工作流。");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("新建环境"),
        ActionsEnvironmentMessage::Name => text.literal("名称"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("添加环境"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(" 位审查者");
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("受保护分支"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("分支 ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("未设保护"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("保护规则"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("必需的审查者"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("每行一个用户名"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal(
            "最多六位可读取此仓库的用户。此环境中的作业会等待其中一位批准；被拒绝的作业会失败。若未设置审查者，作业将无需审查直接运行。",
        ),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("禁止自我审查"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => text.literal("启动运行的人不能批准该运行的部署。"),
        ActionsEnvironmentMessage::WaitTimer => text.literal("等待计时器"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal(
            "此环境中的作业在就绪并获得批准后、开始运行前需要等待的分钟数。0 表示不等待；最多 43200（30 天）。",
        ),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("部署分支和标记"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("任意分支或标记"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("仅限受保护分支"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal("设有分支保护规则的分支。"),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("选定的分支和标记"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("分支和标记模式"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("每行一个模式，与工作流筛选器的写法相同。分支直接写名称（");
            text.argument(branch);
            text.literal("），标记使用 ");
            text.argument(tags);
            text.literal("（");
            text.argument(tag);
            text.literal("）；");
            text.argument(exclude);
            text.literal(" 表示排除。");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("保存保护规则"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("删除此环境"),
        ActionsEnvironmentMessage::DeleteHint => {
            text.literal("其机密、变量和规则也会一并删除。正在等待其审查者的作业将立即运行。");
        }
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("删除环境"),
    }
}
