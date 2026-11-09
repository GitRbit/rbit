//! Simplified Chinese (`zh-Hans`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;

/// A revision number as the English arm writes it.
fn revision(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

entries! { ZhHans:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("Wiki"),
        WikiMessage::Pages => text.literal("页面"),
        WikiMessage::AllPages => text.literal("所有页面"),
        WikiMessage::PagesLabel => text.literal("Wiki 页面"),
        WikiMessage::BreadcrumbLabel => text.literal("路径导航"),
        WikiMessage::SearchPages => text.literal("搜索页面"),
        WikiMessage::Search => text.literal("搜索"),
        WikiMessage::NewPage => text.literal("新建页面"),
        WikiMessage::EditPage => text.literal("编辑页面"),
        WikiMessage::Edit => text.literal("编辑"),
        WikiMessage::History => text.literal("历史记录"),
        WikiMessage::Delete => text.literal("删除"),
        WikiMessage::LastEdited => text.literal("最后编辑"),
        WikiMessage::Revision { generation } => {
            text.literal("修订 ");
            text.number(revision(generation));
        }
        WikiMessage::RevisionColumn => text.literal("修订"),
        WikiMessage::Author => text.literal("作者"),
        WikiMessage::Date => text.literal("日期"),
        WikiMessage::UnknownAuthor => text.literal("未知作者"),
        WikiMessage::Created => text.literal("已创建"),
        WikiMessage::Edited => text.literal("已编辑"),
        WikiMessage::Renamed => text.literal("已重命名"),
        WikiMessage::Deleted => text.literal("已删除"),
        WikiMessage::Restored => text.literal("已恢复"),
        WikiMessage::View => text.literal("查看"),
        WikiMessage::CompareWithPrevious => text.literal("与上一修订比较"),
        WikiMessage::Restore => text.literal("恢复此修订"),
        WikiMessage::OlderRevisions => text.literal("更早的修订"),
        WikiMessage::OldRevisionNotice => text.literal("你正在查看此页面的旧修订。"),
        WikiMessage::ViewCurrent => text.literal("查看当前页面"),
        WikiMessage::HistoryOf { title } => {
            text.literal("“");
            text.argument(title);
            text.literal("”的历史记录");
        }
        WikiMessage::Comparing { base, head } => {
            text.literal("从修订 ");
            text.number(revision(base));
            text.literal(" 到修订 ");
            text.number(revision(head));
            text.literal(" 的更改");
        }
        WikiMessage::Changes { additions, deletions } => {
            text.literal("新增 ");
            text.number(additions);
            text.literal(" 行，删除 ");
            text.number(deletions);
            text.literal(" 行");
        }
        WikiMessage::NoDifferences => text.literal("这两个修订的内容相同。"),
        WikiMessage::Title => text.literal("标题"),
        WikiMessage::Path => text.literal("路径"),
        WikiMessage::PathHint => text.literal("字母、数字、- _ 和 .，以 / 分隔。留空则根据标题生成。"),
        WikiMessage::Content => text.literal("内容"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("保存页面"),
        WikiMessage::Preview => text.literal("预览"),
        WikiMessage::NothingToPreview => text.literal("没有可预览的内容。"),
        WikiMessage::Cancel => text.literal("取消"),
        WikiMessage::TitleEmpty => text.literal("请输入标题。"),
        WikiMessage::TitleTooLong => text.literal("标题太长。"),
        WikiMessage::TitleInvalid => text.literal("标题包含不允许使用的字符。"),
        WikiMessage::PathEmpty => text.literal("请输入路径，或输入可用于生成路径的标题。"),
        WikiMessage::PathTooLong => text.literal("路径或其中某一部分太长。"),
        WikiMessage::PathTooDeep => text.literal("路径的层级太多。"),
        WikiMessage::PathInvalid => text.literal("路径的每一部分都必须以字母或数字开头，且只能使用字母、数字、- _ 和 .。"),
        WikiMessage::PathReserved => text.literal("路径使用了保留名称。"),
        WikiMessage::ContentTooLarge => text.literal("内容太大。"),
        WikiMessage::ContentInvalid => text.literal("内容包含不允许使用的字符。"),
        WikiMessage::Conflict => {
            text.literal("你开始编辑后，此页面已被更改。你的文本保留在下方；请先查看当前页面，再重新保存。");
        }
        WikiMessage::CompareChanges => text.literal("比较更改"),
        WikiMessage::PathTaken => text.literal("已有其他页面使用此路径。"),
        WikiMessage::Unchanged => text.literal("没有任何更改。"),
        WikiMessage::DeleteTitle { title } => {
            text.literal("删除“");
            text.argument(title);
            text.literal("”？");
        }
        WikiMessage::DeletePage => text.literal("删除页面"),
        WikiMessage::Disabled => text.literal("此仓库的 Wiki 已关闭。"),
        WikiMessage::About => text.literal("Wiki 用于存放文档页面，这些页面拥有独立于代码的历史记录。"),
        WikiMessage::TurnOn => text.literal("开启 Wiki"),
        WikiMessage::TurnOff => text.literal("关闭 Wiki"),
        WikiMessage::Empty => text.literal("此 Wiki 还没有页面。"),
        WikiMessage::CreateFirst => text.literal("创建第一个页面"),
        WikiMessage::Missing => text.literal("没有使用此路径的页面。"),
        WikiMessage::CreatePath { path } => {
            text.literal("创建 ");
            text.argument(path);
        }
        WikiMessage::SearchCount { count, query } => {
            text.number(count);
            text.literal(" 个页面匹配“");
            text.argument(query);
            text.literal("”");
        }
        WikiMessage::NoResults { query } => {
            text.literal("没有页面匹配“");
            text.argument(query);
            text.literal("”。");
        }
        WikiMessage::MorePages => text.literal("更多页面"),
    }
}
