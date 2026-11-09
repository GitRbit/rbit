//! Japanese (`ja`) entries for `catalog/explore.rs`.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("リポジトリ"),
        Message::RepositoryCount { count } => {
            text.number(count);
            text.literal(" 件のリポジトリ");
        }
        Message::OwnerRepositories { owner } => {
            text.argument(owner);
            text.literal(" のリポジトリ");
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "パブリック",
            RepositoryVisibility::Private => "プライベート",
            RepositoryVisibility::Internal => "内部",
        }),
        Message::EmptyRepositories => text.literal("リポジトリが見つかりませんでした。"),
        Message::SearchRepositories => text.literal("リポジトリを検索"),
        Message::SearchLabel => text.literal("検索"),
        Message::NextPage => text.literal("次へ"),
        Message::PreviousPage => text.literal("前へ"),
        Message::PageNumber { page } => {
            text.number(page);
            text.literal(" ページ");
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.literal("リポジトリ ");
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" を開く");
        }
        Message::ExploreTitle => text.literal("探索"),
        Message::ExploreSubtitle { site } => {
            text.argument(site);
            text.literal(" のパブリックリポジトリです。");
        }
        Message::ExploreSearchPlaceholder => text.literal("名前または説明で検索"),
        Message::SortBy => text.literal("並べ替え"),
        Message::SortUpdated => text.literal("更新日"),
        Message::SortName => text.literal("名前"),
        Message::SortNewest => text.literal("新しい順"),
        Message::ArchiveFilterLabel => text.literal("アーカイブ済みのリポジトリ"),
        Message::ArchiveAll => text.literal("すべて"),
        Message::ArchiveActive => text.literal("アクティブ"),
        Message::Archived => text.literal("アーカイブ済み"),
        Message::NothingHereYet => text.literal("まだ何もありません"),
        Message::PublicRepositoriesEmpty => text.literal("パブリックリポジトリが作成されると、ここに表示されます。"),
        Message::NoMatchingRepositories => text.literal("一致するリポジトリはありません"),
        Message::SearchSuggestion => text.literal("短い検索語や別の言葉で試してください。"),
        Message::CountForQuery { count, query } => {
            if query.is_empty() {
                text.number(count);
                text.literal(" 件のリポジトリ");
            } else {
                text.literal("「");
                text.argument(query);
                text.literal("」に一致するリポジトリ：");
                text.number(count);
                text.literal(" 件");
            }
        }
        Message::PaginationLabel => text.literal("ページ送り"),
        Message::PageOf { page, pages } => {
            text.number(page.into());
            text.literal(" / ");
            text.number(pages.into());
            text.literal(" ページ");
        }
        Message::Stars { count } => {
            signed(text, count);
            text.literal(" スター");
        }
    }
}
