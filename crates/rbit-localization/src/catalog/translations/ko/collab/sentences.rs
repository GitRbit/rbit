//! Korean (`ko`) entries for `catalog/collab/sentences.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::collab::sentences::CollabSentence;

entries! { Ko:
    CollabSentence<'a> => RichWriter |text| {
        CollabSentence::OpenCount { .. } => {
            text.slot(0);
            text.literal("개 열림");
        }
        CollabSentence::ClosedCount { .. } => {
            text.slot(0);
            text.literal("개 닫힘");
        }
        CollabSentence::IssueSearchHelp => {
            text.literal("메뉴로 결과를 필터링하거나 ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(", ");
            text.slot(2);
            text.literal(", ");
            text.slot(3);
            text.literal(" 같은 조건을 입력하세요. 텍스트는 제목과 설명에서 검색됩니다. 각 메뉴는 다른 필터를 그대로 유지합니다.");
        }
        CollabSentence::PullSearchHelp => {
            text.literal("아래에서 필터를 선택하거나 ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(", ");
            text.slot(2);
            text.literal(", ");
            text.slot(3);
            text.literal(" 같은 조건을 입력하세요.");
        }
        CollabSentence::OpenedIssue { comments, .. } => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("에 이 이슈를 열었습니다 · 댓글 ");
            text.number(comments);
            text.literal("개");
        }
        CollabSentence::OpenedPull { comments, .. } => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(1);
            text.literal("에 이 풀 리퀘스트를 열었습니다 · 댓글 ");
            text.number(comments);
            text.literal("개");
        }
        CollabSentence::RowOpened { number, author, .. } => {
            text.argument(number);
            text.literal(" · ");
            text.argument(author);
            text.literal(" 님이 ");
            text.slot(0);
            text.literal("에 열었음");
        }
        CollabSentence::RowClosed { number, .. } => {
            text.argument(number);
            text.literal(" · ");
            text.slot(0);
            text.literal("에 닫힘");
        }
        CollabSentence::RowClosedBy { number, author, .. } => {
            text.argument(number);
            text.literal(" · ");
            text.argument(author);
            text.literal(" 님이 ");
            text.slot(0);
            text.literal("에 닫았음");
        }
        CollabSentence::RowMergedBy { number, author, .. } => {
            text.argument(number);
            text.literal(" · ");
            text.argument(author);
            text.literal(" 님이 ");
            text.slot(0);
            text.literal("에 병합했음");
        }
        CollabSentence::NoOneAssignYourself => {
            text.literal("없음 — ");
            text.slot(0);
        }
        CollabSentence::PercentComplete => {
            text.slot(0);
            text.literal(" 완료");
        }
        CollabSentence::WantsToMerge => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(3);
            text.literal("의 ");
            text.slot(1);
            text.literal("을(를) ");
            text.slot(2);
            text.literal("에 병합하려고 합니다");
        }
        CollabSentence::MergedInto => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(3);
            text.literal("의 ");
            text.slot(1);
            text.literal("을(를) ");
            text.slot(2);
            text.literal("에 병합했습니다");
        }
        CollabSentence::ProposedToMerge => {
            text.slot(0);
            text.literal(" 님이 ");
            text.slot(3);
            text.literal("의 ");
            text.slot(1);
            text.literal("을(를) ");
            text.slot(2);
            text.literal("에 병합하도록 제안했습니다");
        }
        CollabSentence::SignInToOpenPull => {
            text.slot(0);
            text.literal(" 후 풀 리퀘스트를 열 수 있습니다.");
        }
        CollabSentence::SignInToCommentOnLines => {
            text.slot(0);
            text.literal(" 후 줄에 댓글을 달 수 있습니다.");
        }
        CollabSentence::SignInToJoin => {
            text.slot(0);
            text.literal(" 후 대화에 참여할 수 있습니다.");
        }
        CollabSentence::PullMergedAs => {
            text.literal("풀 리퀘스트가 ");
            text.slot(0);
            text.literal("(으)로 병합되었습니다.");
        }
        CollabSentence::FilesChanged { .. } => {
            text.literal("변경된 파일 ");
            text.slot(0);
            text.literal("개");
        }
        CollabSentence::CommentOpened => {
            text.slot(0);
            text.literal("에 작성함");
        }
        CollabSentence::CommentCommented => {
            text.slot(0);
            text.literal("에 댓글 남김");
        }
        CollabSentence::ReviewApproved => {
            text.slot(0);
            text.literal(" 님이 이 변경 사항을 승인했습니다");
        }
        CollabSentence::ReviewChangesRequested => {
            text.slot(0);
            text.literal(" 님이 변경을 요청했습니다");
        }
        CollabSentence::ReviewDismissed => {
            text.slot(0);
            text.literal(" 님이 리뷰했습니다(기각됨)");
        }
        CollabSentence::Reviewed => {
            text.slot(0);
            text.literal(" 님이 리뷰했습니다");
        }
    }
}
