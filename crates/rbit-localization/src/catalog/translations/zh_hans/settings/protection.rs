//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/protection.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::ProtectionSentence;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};

entries! { ZhHans:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("请勾选复选框以确认。"),
        ProtectionMessage::Settings => text.literal("设置"),
        ProtectionMessage::Title => text.literal("分支保护"),
        ProtectionMessage::AddRule => text.literal("添加规则"),
        ProtectionMessage::Deleted => text.literal("规则已删除。"),
        ProtectionMessage::Empty => text.literal("没有受保护分支"),
        ProtectionMessage::EmptyText => {
            text.literal("添加一条规则，防止 main 等分支被强制推送、删除，或在没有拉取请求的情况下被修改。");
        }
        ProtectionMessage::NoMatchYet => text.literal("尚未匹配任何分支。"),
        ProtectionMessage::MatchesLabel => text.literal("匹配："),
        ProtectionMessage::NewRule => text.literal("新建分支保护规则"),
        ProtectionMessage::NewRuleIntro => text.literal("规则适用于其模式匹配的所有分支，包括现在和以后的分支。"),
        ProtectionMessage::Saved => text.literal("规则已保存。"),
        ProtectionMessage::Rule => text.literal("规则"),
        ProtectionMessage::Pattern => text.literal("分支名称模式"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("分支名称或模式："),
        ProtectionMessage::PatternOne => text.literal("匹配名称中的一个部分，"),
        ProtectionMessage::PatternAcross => text.literal("可跨越多个部分（"),
        ProtectionMessage::PatternChar => text.literal("），"),
        ProtectionMessage::PatternTail => text.literal("匹配单个字符。区分大小写。"),
        ProtectionMessage::Protect => text.literal("保护匹配的分支"),
        ProtectionMessage::BlockForcePush => text.literal("禁止强制推送"),
        ProtectionMessage::BlockForcePushText => text.literal("推送只能添加提交；不能改写历史记录。"),
        ProtectionMessage::BlockDeletion => text.literal("禁止删除"),
        ProtectionMessage::BlockDeletionText => text.literal("匹配的分支不能被删除。"),
        ProtectionMessage::RequirePullRequest => text.literal("要求拉取请求"),
        ProtectionMessage::RequirePullRequestText => {
            text.literal("更改只能通过合并拉取请求进入；直接推送到现有分支会被拒绝。");
        }
        ProtectionMessage::RequiredApprovals => text.literal("所需批准数"),
        ProtectionMessage::DismissStale => text.literal("推送新提交时驳回已有批准"),
        ProtectionMessage::BlockOnChanges => text.literal("有更改请求时禁止合并"),
        ProtectionMessage::RestrictPushers => text.literal("限制可推送的人员"),
        ProtectionMessage::RestrictPeople => text.literal("只有下列人员可以推送或合并。部署密钥始终不可以。"),
        ProtectionMessage::RestrictPeopleTeams => text.literal("只有下列人员和团队可以推送或合并。部署密钥始终不可以。"),
        ProtectionMessage::People => text.literal("人员"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal("用空格或逗号分隔的用户名。每个人都需要拥有写入权限。"),
        ProtectionMessage::Teams => text.literal("团队"),
        ProtectionMessage::NoTeams => text.literal("此组织还没有团队。"),
        ProtectionMessage::RequireChecks => text.literal("要求状态检查通过"),
        ProtectionMessage::RequireChecksText => {
            text.literal("只有当头部提交上下列每项检查的最新状态均为成功时，拉取请求才能合并。");
        }
        ProtectionMessage::RequiredChecks => text.literal("必需的检查"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("允许管理员绕过"),
        ProtectionMessage::AdminBypassText => {
            text.literal("仓库管理员不受此规则约束。每次绕过都会记录在审计日志中。");
        }
        ProtectionMessage::SaveRule => text.literal("保存规则"),
        ProtectionMessage::DangerZone => text.literal("危险区域"),
        ProtectionMessage::DeleteTitle => text.literal("删除此规则"),
        ProtectionMessage::DeleteLead => text.literal("匹配"),
        ProtectionMessage::DeleteTail => text.literal("的分支将立即失去保护。"),
        ProtectionMessage::DeleteConfirm => text.literal("我了解此规则将被删除"),
        ProtectionMessage::DeleteRule => text.literal("删除规则"),
        ProtectionMessage::NoMatches => text.literal("尚未匹配任何分支"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("此仓库的规则已达上限 ");
            text.number(max);
            text.literal(" 条。请先删除一条再添加。");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal("每行一个上下文，与 CI 报告的名称完全一致（区分大小写），最多 ");
            text.number(max);
            text.literal(" 个。");
        }
        ProtectionText::Matches(names) => {
            text.literal("匹配 ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("匹配 ");
            text.argument(names);
            text.literal(" 及另外 ");
            text.number(more);
            text.literal(" 个");
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("分支保护 · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("新建分支保护规则 · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · 分支保护 · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("禁止强制推送"),
        RuleBadge::NoDeletion => text.literal("禁止删除"),
        RuleBadge::PullRequest => text.literal("拉取请求"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(" 个批准");
        }
        RuleBadge::StatusChecks => text.literal("状态检查"),
        RuleBadge::Restricted => text.literal("受限"),
        RuleBadge::AdminsBypass => text.literal("管理员可绕过"),
    }
    ProtectionSentence => RichWriter |text| {
        ProtectionSentence::Intro => {
            text.literal("规则适用于通过 HTTP 和 SSH 对 ");
            text.slot(0);
            text.literal(" 的每次推送，包括管理员，除非规则允许其绕过。");
        }
    }
}
