//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};

/// "3 次提交".
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(u64::try_from(count).unwrap_or(u64::MAX));
    text.literal(" 次提交");
}

entries! { ZhHans:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("提交动态"),
        ContributionLabel::Loading => text.literal("正在加载提交动态…"),
        ContributionLabel::ViewCommitActivity => text.literal("查看提交动态"),
        ContributionLabel::Year => text.literal("年份"),
        ContributionLabel::View => text.literal("查看"),
        ContributionLabel::AnnualCalendar => text.literal("全年提交日历"),
        ContributionLabel::QuarterlyCalendar => text.literal("按季度的提交日历"),
        ContributionLabel::Monday => text.literal("周一"),
        ContributionLabel::Wednesday => text.literal("周三"),
        ContributionLabel::Friday => text.literal("周五"),
        ContributionLabel::DatesUseUtc => text.literal("日期使用 UTC。"),
        ContributionLabel::IntensityLegend => text.literal("提交强度，从少到多"),
        ContributionLabel::Less => text.literal("少"),
        ContributionLabel::More => text.literal("多"),
        ContributionLabel::Partial => {
            text.literal("动态不完整：至少有一个仓库超过了 100,000 次提交的历史记录扫描上限。");
        }
        ContributionLabel::NoCommits => text.literal("这一年没有匹配的提交。"),
        ContributionLabel::SelectDate => text.literal("选择一个日期以查看当天的提交动态。"),
        ContributionLabel::Commits => text.literal("提交"),
        ContributionLabel::ActiveDays => text.literal("活跃天数"),
        ContributionLabel::LongestStreak => text.literal("最长连续天数"),
        ContributionLabel::Repositories => text.literal("仓库"),
        ContributionLabel::HowCounted => text.literal("月度动态及提交的统计方式"),
        ContributionLabel::HowCountedText => text.literal(
            "统计包括从所配置的默认分支可达的提交，按作者日期计入。多个仓库共有的相同提交只计一次。\
             仅包括你有权访问的仓库。不包括议题、拉取请求和审查。",
        ),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            text.argument(&year.to_string());
            text.literal(" 年共 ");
            commits(text, count);
        }
        ContributionMessage::ActivityInYear { year } => {
            text.argument(&year.to_string());
            text.literal(" 年的提交动态");
        }
        ContributionMessage::Description { count, days } => {
            text.literal("在 ");
            text.number(u64::try_from(days).unwrap_or(u64::MAX));
            text.literal(" 个活跃日中共有 ");
            commits(text, count);
            text.literal("。颜色越深表示提交越多。聚焦或悬停在某一天上可查看其数量，选择某一天可查看详情。");
        }
        ContributionMessage::Day { count, date } => {
            text.argument(date);
            text.literal(" 有 ");
            commits(text, count);
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.argument(&year.to_string());
            text.literal(" 年 ");
            text.argument(quarter);
            text.literal(" 提交动态");
        }
        ContributionMessage::Days { count } => {
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" 天");
        }
    }
}
