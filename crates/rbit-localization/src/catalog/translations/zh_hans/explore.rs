//! Simplified Chinese (`zh-Hans`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;

entries! { ZhHans:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("仓库"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(" 个仓库");
        }
        Message::OwnerRepositories { owner } => {
            text.argument(owner);
            text.literal(" 的仓库");
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "公开",
            RepositoryVisibility::Private => "私有",
            RepositoryVisibility::Internal => "内部",
        }),
        Message::EmptyRepositories => text.literal("未找到仓库。"),
        Message::SearchRepositories => text.literal("搜索仓库"),
        Message::SearchLabel => text.literal("搜索"),
        Message::NextPage => text.literal("下一页"),
        Message::PreviousPage => text.literal("上一页"),
        Message::PageNumber { page } => {
            text.literal("第 ");
            text.number(page);
            text.literal(" 页");
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("打开仓库 ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
        }
        Message::ExploreTitle => text.literal("探索"),
        Message::ExploreSubtitle { site } => {
            text.argument(site);
            text.literal(" 上的公开仓库。");
        }
        Message::ExploreSearchPlaceholder => text.literal("按名称或描述搜索"),
        Message::SortBy => text.literal("排序方式"),
        Message::SortUpdated => text.literal("最近更新"),
        Message::SortName => text.literal("名称"),
        Message::SortNewest => text.literal("最新"),
        Message::ArchiveFilterLabel => text.literal("已归档的仓库"),
        Message::ArchiveAll => text.literal("全部"),
        Message::ArchiveActive => text.literal("活跃"),
        Message::Archived => text.literal("已归档"),
        Message::NothingHereYet => text.literal("这里还没有内容"),
        Message::PublicRepositoriesEmpty => text.literal("公开仓库创建后会列在这里。"),
        Message::NoMatchingRepositories => text.literal("没有匹配的仓库"),
        Message::SearchSuggestion => text.literal("请尝试更短的搜索词或换个说法。"),
        Message::CountForQuery { count, query } => {
            text.number(count);
            text.literal(" 个仓库");
            if !query.is_empty() {
                text.literal("匹配“");
                text.argument(query);
                text.literal("”");
            }
        }
        Message::PaginationLabel => text.literal("分页"),
        Message::PageOf { page, pages } => {
            text.literal("第 ");
            text.number(page.into());
            text.literal(" 页，共 ");
            text.number(pages.into());
            text.literal(" 页");
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(" 个星标");
        }
    }
}
