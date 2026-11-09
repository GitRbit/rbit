//! Korean (`ko`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};

entries! { Ko:
    RepoHistoryLabel => MessageWriter |text| {
        RepoHistoryLabel::BrowseFilesAtCommit => text.literal("이 커밋의 파일 찾아보기"),
        RepoHistoryLabel::BrowseFiles => text.literal("파일 찾아보기"),
        RepoHistoryLabel::Pagination => text.literal("페이지 이동"),
        RepoHistoryLabel::Newer => text.literal("최신"),
        RepoHistoryLabel::Older => text.literal("이전"),
        RepoHistoryLabel::Committed => text.literal("커밋함"),
        RepoHistoryLabel::CommittedBy => text.literal("커미터:"),
        RepoHistoryLabel::Commit => text.literal("커밋"),
        RepoHistoryLabel::Checks => text.literal("검사"),
        RepoHistoryLabel::ViewFile => text.literal("파일 보기"),
        RepoHistoryLabel::PathInThatCommit => text.literal("해당 커밋의 경로"),
        RepoHistoryLabel::FileEmpty => text.literal("이 파일은 비어 있습니다."),
        RepoHistoryLabel::BinaryCannotBeBlamed => text.literal("바이너리 파일은 blame할 수 없습니다."),
        RepoHistoryLabel::TooLargeToBlame => text.literal("이 파일은 너무 커서 blame할 수 없습니다."),
        RepoHistoryLabel::ViewTheFile => text.literal("파일 보기"),
        RepoHistoryLabel::CompareChanges => text.literal("변경 사항 비교"),
        RepoHistoryLabel::CompareLead => text.literal("베이스와, 이와 비교할 브랜치, 태그 또는 커밋을 선택하세요. 포크 간 비교도 가능합니다."),
        RepoHistoryLabel::Comparison => text.literal("비교"),
        RepoHistoryLabel::MergeBase => text.literal("병합 베이스"),
        RepoHistoryLabel::Swap => text.literal("바꾸기"),
        RepoHistoryLabel::CreatePullRequest => text.literal("풀 리퀘스트 만들기"),
        RepoHistoryLabel::UnrelatedHistories => text.literal("두 리비전의 기록이 완전히 달라 표시할 diff가 없습니다."),
        RepoHistoryLabel::NothingToCompare => text.literal("비교할 내용 없음"),
        RepoHistoryLabel::NothingToCompareText => text.literal("비교한 리비전의 모든 내용이 이미 베이스에 있습니다."),
        RepoHistoryLabel::Commits => text.literal("커밋"),
        RepoHistoryLabel::Compare => text.literal("비교"),
        RepoHistoryLabel::CompareWithDefault => text.literal("기본 브랜치와 비교"),
        RepoHistoryLabel::BranchView => text.literal("브랜치 보기"),
        RepoHistoryLabel::List => text.literal("목록"),
        RepoHistoryLabel::GitTree => text.literal("Git 트리"),
        RepoHistoryLabel::NoMatchingRefs => text.literal("일치하는 참조가 없습니다."),
        RepoHistoryLabel::NoBranchesYet => text.literal("아직 브랜치가 없습니다"),
        RepoHistoryLabel::NoBranchesText => text.literal("커밋을 푸시하면 첫 번째 브랜치가 만들어집니다."),
        RepoHistoryLabel::NoTagsYet => text.literal("아직 태그가 없습니다"),
        RepoHistoryLabel::NoTagsText => text.literal("태그는 릴리스와 기록의 중요한 지점을 표시합니다."),
        RepoHistoryLabel::Protected => text.literal("보호됨"),
        RepoHistoryLabel::ProtectedTitle => text.literal("이 브랜치에는 브랜치 보호 규칙이 적용됩니다"),
        RepoHistoryLabel::DownloadTarGz => text.literal("tar.gz 다운로드"),
        RepoHistoryLabel::BranchCommitGraph => text.literal("브랜치 커밋 그래프"),
        RepoHistoryLabel::GraphHelp => text.literal("색이 있는 선을 따라가며 기록을 추적하세요. 트랙을 선택하면 해당 커밋이 강조 표시됩니다."),
        RepoHistoryLabel::ClearHighlight => text.literal("강조 표시 지우기"),
        RepoHistoryLabel::HistoryConnections => text.literal("기록 연결"),
        RepoHistoryLabel::CommitDetails => text.literal("커밋 세부 정보"),
        RepoHistoryLabel::Merge => text.literal("병합"),
        RepoHistoryLabel::GraphTruncated => text.literal("모든 브랜치에서 최근 커밋 200개를 표시합니다. 아래쪽 선은 이전 기록으로 이어집니다. 더 살펴보려면 브랜치의 기록을 여세요."),
        RepoHistoryLabel::LinesAdded => text.literal("추가됨"),
        RepoHistoryLabel::LinesDeleted => text.literal("삭제됨"),
        RepoHistoryLabel::StatusAdded => text.literal("추가됨"),
        RepoHistoryLabel::StatusDeleted => text.literal("삭제됨"),
        RepoHistoryLabel::StatusModified => text.literal("수정됨"),
        RepoHistoryLabel::StatusRenamed => text.literal("이름 변경됨"),
        RepoHistoryLabel::StatusCopied => text.literal("복사됨"),
        RepoHistoryLabel::ChangedFiles => text.literal("변경된 파일"),
        RepoHistoryLabel::FileTree => text.literal("파일 트리"),
        RepoHistoryLabel::FilterChangedFiles => text.literal("변경된 파일 필터"),
        RepoHistoryLabel::FilterFiles => text.literal("파일 필터…"),
        RepoHistoryLabel::NoFilesMatch => text.literal("일치하는 파일이 없습니다."),
        RepoHistoryLabel::Changes => text.literal("변경 사항"),
        RepoHistoryLabel::ToggleFileTree => text.literal("파일 트리 전환"),
        RepoHistoryLabel::CollapseAll => text.literal("모두 접기"),
        RepoHistoryLabel::ExpandAll => text.literal("모두 펼치기"),
        RepoHistoryLabel::DiffTruncated => text.literal("변경 사항이 너무 커서 일부 diff는 표시되지 않습니다. 전체 변경 사항을 보려면 저장소를 클론하세요."),
        RepoHistoryLabel::CollapseHint => text.literal("클릭하여 접거나 펼치기 · Alt 키를 누른 채 클릭하면 모든 파일에 적용"),
        RepoHistoryLabel::CopyPath => text.literal("경로 복사"),
        RepoHistoryLabel::ViewFileAtCommit => text.literal("이 커밋의 파일 보기"),
        RepoHistoryLabel::View => text.literal("보기"),
        RepoHistoryLabel::BinaryNotShown => text.literal("바이너리 파일은 표시되지 않습니다."),
        RepoHistoryLabel::DiffTooLarge => text.literal("diff가 너무 커서 표시할 수 없습니다."),
        RepoHistoryLabel::SubmoduleCommit => text.literal("서브모듈 커밋"),
        RepoHistoryLabel::NoContentChanges => text.literal("내용 변경 없음."),
    }
    RepoHistoryMessage<'a> => MessageWriter |text| {
        RepoHistoryMessage::CommitsOn { date, .. } => {
            text.argument(date);
            text.literal("의 커밋");
        }
        RepoHistoryMessage::ViewCommit { id, .. } => {
            text.literal("커밋 ");
            text.argument(id);
            text.literal(" 보기");
        }
        RepoHistoryMessage::ParentNoun { .. } => text.literal("부모"),
        RepoHistoryMessage::BlameOf { file, .. } => {
            text.argument(file);
            text.literal("의 blame");
        }
        RepoHistoryMessage::BlameTitle { path, .. } => {
            text.argument(path);
            text.literal(" blame");
        }
        RepoHistoryMessage::Ahead { count, .. } => {
            text.argument(count);
            text.literal("개 앞섬");
        }
        RepoHistoryMessage::Behind { count, .. } => {
            text.argument(count);
            text.literal("개 뒤처짐");
        }
        RepoHistoryMessage::OnlyNewest { count, .. } => {
            text.literal("최신 커밋 ");
            text.number(count as u64);
            text.literal("개만 표시됩니다.");
        }
        RepoHistoryMessage::HighlightTrack { track, .. } => {
            text.literal("트랙 ");
            text.number(track as u64);
            text.literal(" 강조 표시");
        }
        RepoHistoryMessage::Track { track, .. } => {
            text.literal("트랙 ");
            text.number(track as u64);
        }
        RepoHistoryMessage::BranchName { name, .. } => {
            text.literal("브랜치: ");
            text.argument(name);
        }
        RepoHistoryMessage::MoreBranches { count, .. } => {
            text.literal("브랜치 ");
            text.number(count as u64);
            text.literal("개 더");
        }
        RepoHistoryMessage::FilesChangedNoun { .. } => text.literal("파일 변경됨"),
        RepoHistoryMessage::CompareTitle { base, head, .. } => {
            text.literal("비교 ");
            text.argument(base);
            text.literal("...");
            text.argument(head);
        }
    }
}
