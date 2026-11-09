//! Korean (`ko`) entries for `catalog/collab/pulls.rs`.

use crate::catalog::collab::pulls::PullMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    PullMessage<'a> => MessageWriter |text| {
        PullMessage::PullRequests => text.literal("풀 리퀘스트"),
        PullMessage::SearchPulls => text.literal("풀 리퀘스트 검색"),
        PullMessage::ClearSearch => text.literal("검색 및 필터 초기화"),
        PullMessage::FiltersLabel => text.literal("풀 리퀘스트 필터"),
        PullMessage::StateNav => text.literal("풀 리퀘스트 상태"),
        PullMessage::AuthorFilter { author, .. } => {
            text.literal("작성자: ");
            text.argument(author);
        }
        PullMessage::New => text.literal("새"),
        PullMessage::NewSuffix => text.literal("\u{a0}풀 리퀘스트"),
        PullMessage::EmptyTitle => text.literal("풀 리퀘스트가 없습니다"),
        PullMessage::EmptyText => text.literal("풀 리퀘스트는 브랜치나 포크의 변경 사항을 제안합니다. 브랜치를 비교하여 풀 리퀘스트를 여세요."),
        PullMessage::OpenTitle => text.literal("풀 리퀘스트 열기"),
        PullMessage::OpenIntro => text.literal("이 저장소나 포크에 있는 브랜치의 변경 사항을 베이스 브랜치에 제안합니다."),
        PullMessage::OpenAsDraft => text.literal("초안으로 열기"),
        PullMessage::Create => text.literal("풀 리퀘스트 만들기"),
        PullMessage::DifferentHistories => text.literal("이 브랜치들은 기록이 완전히 다릅니다."),
        PullMessage::ChooseBranches => text.literal("비교할 기존 브랜치 두 개를 선택하세요."),
        PullMessage::UpToDate { base, head, .. } => {
            text.literal("베이스 ");
            text.argument(base);
            text.literal("에 헤드 ");
            text.argument(head);
            text.literal("의 모든 커밋이 이미 반영되어 있습니다.");
        }
        PullMessage::BaseRepository => text.literal("베이스 저장소"),
        PullMessage::BaseRefs => text.literal("베이스 브랜치 및 태그"),
        PullMessage::Base => text.literal("베이스"),
        PullMessage::HeadRepository => text.literal("헤드 저장소"),
        PullMessage::Compare => text.literal("비교"),
        PullMessage::CompareRefs => text.literal("브랜치 및 태그 비교"),
        PullMessage::PullRequest => text.literal("풀 리퀘스트"),
        PullMessage::Conversation => text.literal("대화"),
        PullMessage::Commits => text.literal("커밋"),
        PullMessage::FilesChanged => text.literal("변경된 파일"),
        PullMessage::CommitsPhrase { count: Some(count) } => {
            text.literal("커밋 ");
            text.number(count);
            text.literal("개");
        }
        PullMessage::CommitsPhrase { count: None } => text.literal("커밋"),
        PullMessage::ChangeBase => text.literal("베이스 브랜치 변경"),
        PullMessage::BaseBranch => text.literal("베이스 브랜치"),
        PullMessage::BaseBranches => text.literal("베이스 브랜치"),
        PullMessage::ChangeBaseHint => text.literal("베이스를 변경하면 커밋, diff, 병합 검사가 업데이트됩니다."),
        PullMessage::UpdateBase => text.literal("베이스 브랜치 업데이트"),
        PullMessage::IsClosed => text.literal("이 풀 리퀘스트는 닫혔습니다."),
        PullMessage::IsMerged => text.literal("풀 리퀘스트가 병합되었습니다."),
        PullMessage::DeleteBranch { branch, .. } => {
            text.argument(branch);
            text.literal(" 삭제");
        }
        PullMessage::ConvertToDraft => text.literal("초안으로 전환"),
        PullMessage::NoCommitsTitle => text.literal("커밋 없음"),
        PullMessage::NoCommitsText => text.literal("헤드와 베이스가 같은 커밋을 가리킵니다."),
        PullMessage::NewestCommits { count, .. } => {
            text.literal("최신 커밋 ");
            text.number(count);
            text.literal("개만 표시됩니다.");
        }
    }
}
