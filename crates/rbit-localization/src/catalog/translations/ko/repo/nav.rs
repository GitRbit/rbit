//! Korean (`ko`) entries for `catalog/repo/nav.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::RepoNavSentence;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};

entries! { Ko:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ArchivedNote => text.literal("이 저장소는 보관되었습니다. 읽기 전용이므로 클론, 포크, 스타는 할 수 있지만 변경할 수는 없습니다."),
        RepoNavLabel::Private => text.literal("비공개"),
        RepoNavLabel::Public => text.literal("공개"),
        RepoNavLabel::Repository => text.literal("저장소"),
        RepoNavLabel::Code => text.literal("코드"),
        RepoNavLabel::Issues => text.literal("이슈"),
        RepoNavLabel::PullRequests => text.literal("풀 리퀘스트"),
        RepoNavLabel::Commits => text.literal("커밋"),
        RepoNavLabel::Branches => text.literal("브랜치"),
        RepoNavLabel::Tags => text.literal("태그"),
        RepoNavLabel::Releases => text.literal("릴리스"),
        RepoNavLabel::Activity => text.literal("활동"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("설정"),
        RepoNavLabel::ForkThisRepository => text.literal("이 저장소 포크"),
        RepoNavLabel::Fork => text.literal("포크"),
        RepoNavLabel::Forks => text.literal("포크"),
        RepoNavLabel::Clone => text.literal("클론"),
        RepoNavLabel::CloneUrl => text.literal("클론 URL"),
        RepoNavLabel::CopyCloneUrl => text.literal("클론 URL 복사"),
        RepoNavLabel::SshCloneUrl => text.literal("SSH 클론 URL"),
        RepoNavLabel::CopySshCloneUrl => text.literal("SSH 클론 URL 복사"),
        RepoNavLabel::Copy => text.literal("복사"),
        RepoNavLabel::YourSettings => text.literal("내 설정"),
        RepoNavLabel::YourSettingsEnd => text.literal("."),
        RepoNavLabel::DownloadZip => text.literal("ZIP 다운로드"),
        RepoNavLabel::Star => text.literal("스타"),
        RepoNavLabel::Starred => text.literal("스타 표시함"),
        RepoNavLabel::StarThisRepository => text.literal("이 저장소에 스타 표시"),
        RepoNavLabel::UnstarThisRepository => text.literal("이 저장소의 스타 취소"),
        RepoNavLabel::SignInToStar => text.literal("이 저장소에 스타를 표시하려면 로그인하세요"),
        RepoNavLabel::Stargazers => text.literal("스타를 준 사용자"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count, .. } => {
            text.literal("포크 ");
            text.number(count);
            text.literal("개");
        }
        RepoNavMessage::StargazerCount { count, .. } => {
            text.literal("스타 ");
            text.number(count.unsigned_abs());
            text.literal("개");
        }
    }
    RepoNavSentence => RichWriter |text| {
        RepoNavSentence::ForkedFrom => {
            text.literal("포크 원본: ");
            text.slot(0);
        }
        RepoNavSentence::MirrorOf => {
            text.literal("미러 원본: ");
            text.slot(0);
        }
        RepoNavSentence::SshKeyNote => {
            text.literal("SSH를 사용하려면 다음 위치에 키가 필요합니다: ");
            text.slot(0);
            text.literal(".");
        }
    }
}
