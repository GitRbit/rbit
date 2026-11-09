//! Korean (`ko`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { Ko:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("새 파일"),
        RepoEditLabel::Edit => text.literal("편집:"),
        RepoEditLabel::On => text.literal("브랜치:"),
        RepoEditLabel::NewFileHint => text.literal("디렉터리를 만들려면 경로에 /를 입력하세요."),
        RepoEditLabel::EditHint => text.literal("파일 이름을 바꾸거나 옮기려면 경로를 변경하세요."),
        RepoEditLabel::BackToFile => text.literal("파일로 돌아가기"),
        RepoEditLabel::Path => text.literal("경로"),
        RepoEditLabel::Content => text.literal("내용"),
        RepoEditLabel::CommitChanges => text.literal("변경 사항 커밋"),
        RepoEditLabel::Cancel => text.literal("취소"),
        RepoEditLabel::Delete => text.literal("삭제:"),
        RepoEditLabel::DeleteHint => text.literal("파일은 기록에 남으며, 비게 된 디렉터리는 사라집니다."),
        RepoEditLabel::DeleteFile => text.literal("파일 삭제"),
        RepoEditLabel::UploadFiles => text.literal("파일 업로드"),
        RepoEditLabel::Into => text.literal("대상 디렉터리:"),
        RepoEditLabel::OnBranch => text.literal("· 브랜치:"),
        RepoEditLabel::UploadHint => text.literal("이름이 같은 파일은 교체됩니다."),
        RepoEditLabel::Files => text.literal("파일"),
        RepoEditLabel::NothingCommitted => text.literal("아무것도 커밋되지 않았습니다."),
        RepoEditLabel::ReviewNewCommits => text.literal("새 커밋을 검토하세요"),
        RepoEditLabel::KeptAndCommittedOnTop => text.literal("그런 다음 다시 커밋하세요. 변경 사항은 아래에 보존되어 새 버전 위에 커밋됩니다."),
        RepoEditLabel::BranchGone => text.literal("아무것도 커밋되지 않았습니다. 대신 새 브랜치에 커밋하세요. 변경 사항은 아래에 보존되어 있습니다."),
        RepoEditLabel::CommitChangesLegend => text.literal("변경 사항 커밋"),
        RepoEditLabel::CommitMessage => text.literal("커밋 메시지"),
        RepoEditLabel::ExtendedDescription => text.literal("자세한 설명"),
        RepoEditLabel::Optional => text.literal("선택 사항"),
        RepoEditLabel::CommitDirectlyTo => text.literal("다음 브랜치에 직접 커밋:"),
        RepoEditLabel::NotAllowed => text.literal("허용되지 않음:"),
        RepoEditLabel::NewBranchAndPull => text.literal("새 브랜치를 만들고 풀 리퀘스트 시작"),
        RepoEditLabel::NewBranchName => text.literal("새 브랜치 이름"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size, .. } => {
            text.literal("최대 ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size, .. } => {
            text.literal("파일당 최대 ");
            text.argument(size);
            text.literal(", 최대 ");
            text.number(files as u64);
            text.literal("개 파일을 한 번에 커밋합니다.");
        }
        RepoEditMessage::BranchChanged { branch, .. } => {
            text.literal("작업을 시작한 후 ");
            text.argument(branch);
            text.literal(" 브랜치가 변경되었습니다");
        }
        RepoEditMessage::BranchMissing { branch, .. } => {
            text.argument(branch);
            text.literal(" 브랜치가 더 이상 존재하지 않습니다");
        }
        RepoEditMessage::NowPointsAt => text.literal("현재 가리키는 커밋:"),
        RepoEditMessage::NotEditable { limit, .. } => {
            text.literal("이 파일은 바이너리이거나 ");
            text.argument(limit);
            text.literal("보다 커서 브라우저에서 편집할 수 없습니다. 삭제하거나 대체 파일을 업로드할 수 있습니다.");
        }
        RepoEditMessage::EditingTitle { path, .. } => {
            text.argument(path);
            text.literal(" 편집 중");
        }
        RepoEditMessage::DeleteTitle { path, .. } => {
            text.argument(path);
            text.literal(" 삭제");
        }
    }
}
