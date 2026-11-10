//! Korean (`ko`) entries for `catalog/repo/browse.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::RepoBrowseSentence;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};

entries! { Ko:
    RepoBrowseLabel => MessageWriter |text| {
        RepoBrowseLabel::About => text.literal("정보"),
        RepoBrowseLabel::NoDescription => text.literal("설명이 없습니다."),
        RepoBrowseLabel::Visibility => text.literal("공개 범위"),
        RepoBrowseLabel::History => text.literal("기록"),
        RepoBrowseLabel::CommitHistory => text.literal("커밋 기록"),
        RepoBrowseLabel::AddFile => text.literal("파일 추가"),
        RepoBrowseLabel::UploadFiles => text.literal("파일 업로드"),
        RepoBrowseLabel::Files => text.literal("파일"),
        RepoBrowseLabel::ParentDirectory => text.literal("상위 디렉터리"),
        RepoBrowseLabel::NameColumn => text.literal("이름"),
        RepoBrowseLabel::LastCommitMessageColumn => text.literal("마지막 커밋 메시지"),
        RepoBrowseLabel::LastCommitDateColumn => text.literal("마지막 커밋 날짜"),
        RepoBrowseLabel::Path => text.literal("경로"),
        RepoBrowseLabel::Latest => text.literal("최신"),
        RepoBrowseLabel::YourAccess => text.literal("내 접근 권한"),
        RepoBrowseLabel::LeaveRepository => text.literal("저장소 나가기"),
        RepoBrowseLabel::Languages => text.literal("언어"),
        RepoBrowseLabel::RepositoryLinks => text.literal("저장소 링크"),
        RepoBrowseLabel::StoredWithLfs => text.literal("Git LFS에 저장됨"),
        RepoBrowseLabel::LfsObjectMissing => text.literal("이 서버에 없는 객체"),
        RepoBrowseLabel::EditThisFile => text.literal("이 파일 편집"),
        RepoBrowseLabel::DeleteThisFile => text.literal("이 파일 삭제"),
        RepoBrowseLabel::Blame => text.literal("Blame"),
        RepoBrowseLabel::Raw => text.literal("원본"),
        RepoBrowseLabel::Download => text.literal("다운로드"),
        RepoBrowseLabel::BinaryNotShown => text.literal("바이너리 파일은 표시되지 않습니다."),
        RepoBrowseLabel::TooLargeToDisplay => text.literal("이 파일은 너무 커서 표시할 수 없습니다."),
        RepoBrowseLabel::ViewRaw => text.literal("원본 보기"),
        RepoBrowseLabel::RepositoryDocuments => text.literal("저장소 문서"),
        RepoBrowseLabel::DocumentOutline => text.literal("문서 개요"),
        RepoBrowseLabel::FilterHeadings => text.literal("제목 필터"),
        RepoBrowseLabel::DocumentHeadings => text.literal("문서 제목"),
        RepoBrowseLabel::NoMatchingHeadings => text.literal("일치하는 제목이 없습니다."),
        RepoBrowseLabel::EmptyTitle => text.literal("이 저장소는 비어 있습니다"),
        RepoBrowseLabel::EmptyLead => text.literal("기존 프로젝트를 푸시하거나 명령줄에서 새 프로젝트를 시작하세요."),
        RepoBrowseLabel::RepositoryUrl => text.literal("저장소 URL"),
        RepoBrowseLabel::CopyRepositoryUrl => text.literal("저장소 URL 복사"),
        RepoBrowseLabel::SshRepositoryUrl => text.literal("SSH 저장소 URL"),
        RepoBrowseLabel::CopySshRepositoryUrl => text.literal("SSH 저장소 URL 복사"),
        RepoBrowseLabel::CreateNewRepository => text.literal("새 저장소 만들기"),
        RepoBrowseLabel::PushExistingRepository => text.literal("기존 저장소 푸시"),
        RepoBrowseLabel::CloseFileBrowser => text.literal("파일 탐색기 닫기"),
        RepoBrowseLabel::LanguageFiles => text.literal("언어별 파일"),
        RepoBrowseLabel::NoFilesForLanguage => text.literal("이 언어로 된 파일이 없습니다."),
        RepoBrowseLabel::NoMatchingFiles => text.literal("일치하는 파일이 없습니다."),
        RepoBrowseLabel::FileContent => text.literal("파일 내용"),
        RepoBrowseLabel::SelectFileToPreview => text.literal("내용을 미리 보려면 파일을 선택하세요."),
        RepoBrowseLabel::SearchThisListing => text.literal("이 목록 검색"),
        RepoBrowseLabel::SearchNames => text.literal("이름 검색"),
        RepoBrowseLabel::Search => text.literal("검색"),
        RepoBrowseLabel::ListingPages => text.literal("목록 페이지"),
        RepoBrowseLabel::Previous => text.literal("이전"),
        RepoBrowseLabel::Next => text.literal("다음"),
        RepoBrowseLabel::BranchesAndTags => text.literal("브랜치 및 태그"),
        RepoBrowseLabel::BrowseBranchesAndTags => text.literal("브랜치 및 태그 찾아보기"),
        RepoBrowseLabel::FindBranchOrTag => text.literal("브랜치 또는 태그 찾기"),
        RepoBrowseLabel::Find => text.literal("찾기"),
        RepoBrowseLabel::Branch => text.literal("브랜치"),
        RepoBrowseLabel::Tag => text.literal("태그"),
        RepoBrowseLabel::Default => text.literal("기본"),
        RepoBrowseLabel::NoMatchingRefs => text.literal("일치하는 브랜치나 태그가 없습니다."),
        RepoBrowseLabel::NoForksYet => text.literal("아직 포크가 없습니다"),
        RepoBrowseLabel::ForksEmpty => text.literal("포크는 다른 사람이 독립적으로 작업하는 이 저장소의 사본입니다."),
        RepoBrowseLabel::ForkLead => text.literal("포크는 저장소의 브랜치와 태그를 복사한 나만의 사본입니다. 원본의 공개 범위를 그대로 유지합니다."),
        RepoBrowseLabel::ForkLeadPrivate => text.literal("포크는 저장소의 브랜치와 태그를 복사한 나만의 사본입니다. 원본의 공개 범위를 그대로 유지하므로 이 포크는 비공개로 유지됩니다."),
        RepoBrowseLabel::Owner => text.literal("소유자"),
        RepoBrowseLabel::Name => text.literal("이름"),
        RepoBrowseLabel::CreateFork => text.literal("포크 만들기"),
        RepoBrowseLabel::Cancel => text.literal("취소"),
        RepoBrowseLabel::NoStargazersYet => text.literal("아직 스타를 준 사용자가 없습니다"),
        RepoBrowseLabel::StargazersEmpty => text.literal("이 저장소에 스타를 표시한 사용자가 여기에 표시됩니다."),
        RepoBrowseLabel::RepositoryActivity => text.literal("저장소 활동"),
        RepoBrowseLabel::GoToFile => text.literal("파일로 이동"),
        RepoBrowseLabel::MatchingFiles => text.literal("일치하는 파일"),
        RepoBrowseLabel::LoadingFiles => text.literal("파일을 불러오는 중…"),
        RepoBrowseLabel::UnableToLoadFiles => text.literal("파일을 불러올 수 없습니다. Enter 키를 눌러 검색하세요."),
        RepoBrowseLabel::SeeAllMatches => text.literal("일치 항목 모두 보기"),
        RepoBrowseLabel::NoFilesInRevision => text.literal("이 리비전에는 파일이 없습니다."),
        RepoBrowseLabel::TooManyFilesToSearch => text.literal("이 리비전에는 파일이 너무 많아 모두 검색할 수 없습니다."),
    }
    RepoBrowseMessage<'a> => MessageWriter |text| {
        RepoBrowseMessage::BranchNoun { .. } => text.literal("브랜치"),
        RepoBrowseMessage::TagNoun { .. } => text.literal("태그"),
        RepoBrowseMessage::CollaboratorAccess { permission, .. } => {
            text.literal("공동 작업자로서 ");
            text.argument(permission);
            text.literal(" 권한이 있습니다.");
        }
        RepoBrowseMessage::LineCount { count, .. } => {
            text.number(count as u64);
            text.literal("줄");
        }
        RepoBrowseMessage::LanguageFiles { language, .. } => {
            text.argument(language);
            text.literal(" 파일");
        }
        RepoBrowseMessage::ListingResults { total, page, pages, .. } => {
            text.literal("결과 ");
            text.number(total as u64);
            text.literal("개 · ");
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
        }
        RepoBrowseMessage::RefPage { page, pages, total, .. } => {
            text.number(page as u64);
            text.literal(" / ");
            text.number(pages as u64);
            text.literal(" · 참조 ");
            text.number(total as u64);
            text.literal("개");
        }
        RepoBrowseMessage::ForkTitle { owner, name, .. } => {
            text.argument(owner);
            text.literal("/");
            text.argument(name);
            text.literal(" 포크");
        }
        RepoBrowseMessage::CommitNoun { .. } => text.literal("커밋"),
        RepoBrowseMessage::NoFileMatches { query } => {
            text.literal("“");
            text.argument(query);
            text.literal("”에 일치하는 파일이 없습니다.");
        }
        RepoBrowseMessage::MoreFileMatches { count } => {
            text.number(count as u64);
            text.literal("개가 더 일치합니다. 이름을 더 입력해 목록을 좁히세요.");
        }
    }
    RepoBrowseSentence => RichWriter |text| {
        RepoBrowseSentence::EmptySshNote => {
            text.literal("아래 명령은 HTTPS를 사용합니다. SSH로 푸시하려면 SSH URL을 대신 사용하고 다음 위치에서 키를 추가하세요: ");
            text.slot(0);
            text.literal(".");
        }
    }
}
