//! Korean (`ko`) entries for `catalog/explore.rs`. Korean has no plural forms.

use crate::catalog::RepositoryVisibility;
use crate::catalog::explore::{Message, signed};
use crate::catalog::fallback::MessageWriter;

/// "저장소 3개", with the counter after the number.
fn repositories(text: &mut MessageWriter, count: u64) {
    text.literal("저장소 ");
    text.number(count);
    text.literal("개");
}

entries! { Ko:
    Message<'a> => MessageWriter |text| {
        Message::RepositoriesTitle => text.literal("저장소"),
        Message::RepositoryCount { count } => repositories(text, count),
        Message::OwnerRepositories { owner } => {
            text.argument(owner);
            text.literal("의 저장소");
        }
        Message::Visibility { visibility } => text.literal(match visibility {
            RepositoryVisibility::Public => "공개",
            RepositoryVisibility::Private => "비공개",
            RepositoryVisibility::Internal => "내부",
        }),
        Message::EmptyRepositories => text.literal("저장소를 찾을 수 없습니다."),
        Message::SearchRepositories => text.literal("저장소 검색"),
        Message::SearchLabel => text.literal("검색"),
        Message::NextPage => text.literal("다음"),
        Message::PreviousPage => text.literal("이전"),
        Message::PageNumber { page } => {
            text.number(page);
            text.literal("페이지");
        }
        Message::RepositoryLinkLabel { owner, name } => {
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" 저장소 열기");
        }
        Message::ExploreTitle => text.literal("탐색"),
        Message::ExploreSubtitle { site } => {
            text.argument(site);
            text.literal("의 공개 저장소입니다.");
        }
        Message::ExploreSearchPlaceholder => text.literal("이름 또는 설명으로 검색"),
        Message::SortBy => text.literal("정렬 기준"),
        Message::SortUpdated => text.literal("최근 업데이트순"),
        Message::SortName => text.literal("이름순"),
        Message::SortNewest => text.literal("최신순"),
        Message::ArchiveFilterLabel => text.literal("보관된 저장소"),
        Message::ArchiveAll => text.literal("전체"),
        Message::ArchiveActive => text.literal("활성"),
        Message::Archived => text.literal("보관됨"),
        Message::NothingHereYet => text.literal("아직 아무것도 없습니다"),
        Message::PublicRepositoriesEmpty => text.literal("공개 저장소가 생기면 여기에 표시됩니다."),
        Message::NoMatchingRepositories => text.literal("일치하는 저장소가 없습니다"),
        Message::SearchSuggestion => text.literal("더 짧은 검색어나 다른 단어로 검색해 보세요."),
        Message::CountForQuery { count, query } => {
            if !query.is_empty() {
                text.literal("“");
                text.argument(query);
                text.literal("” 검색 결과: ");
            }
            repositories(text, count);
        }
        Message::PaginationLabel => text.literal("페이지 매기기"),
        Message::PageOf { page, pages } => {
            text.number(pages.into());
            text.literal("페이지 중 ");
            text.number(page.into());
            text.literal("페이지");
        }
        Message::Stars { count } => {
            text.literal("스타 ");
            signed(text, count);
            text.literal("개");
        }
    }
}
