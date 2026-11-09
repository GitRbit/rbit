//! Korean (`ko`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("변경 사항"),
        ReviewMessage::ToggleFileTree => text.literal("파일 트리 전환"),
        ReviewMessage::CollapseAll => text.literal("모두 접기"),
        ReviewMessage::ExpandAll => text.literal("모두 펼치기"),
        ReviewMessage::OutdatedComments { count, .. } => {
            text.literal("대화에 오래된 댓글 ");
            text.number(count);
            text.literal("개가 있습니다");
        }
        ReviewMessage::FinishReview => text.literal("리뷰 완료하기"),
        ReviewMessage::Truncated => text.literal("변경 사항이 너무 커서 일부 diff는 표시되지 않습니다. 전체 변경 사항을 보려면 저장소를 클론하세요."),
        ReviewMessage::FileToggleHint => text.literal("클릭하여 접거나 펼치기 | Alt 키를 누른 채 클릭하면 모든 파일에 적용"),
        ReviewMessage::ViewFile => text.literal("파일 보기"),
        ReviewMessage::CommentOnLine { line, .. } => {
            text.number(line);
            text.literal("번 줄에 댓글 달기");
        }
        ReviewMessage::StartReview => text.literal("리뷰 시작"),
        ReviewMessage::AddSingleComment => text.literal("단일 댓글 추가"),
        ReviewMessage::BinaryFile => text.literal("바이너리 파일은 표시되지 않습니다."),
        ReviewMessage::TooLarge => text.literal("diff가 너무 커서 표시할 수 없습니다."),
        ReviewMessage::SubmoduleCommit => text.literal("서브모듈 커밋"),
        ReviewMessage::NoContentChanges => text.literal("내용 변경 없음."),
        ReviewMessage::NoChangesTitle => text.literal("변경 사항 없음"),
        ReviewMessage::NoChangesText => text.literal("헤드에 베이스 대비 추가된 내용이 없습니다."),
        ReviewMessage::Added => text.literal("추가됨"),
        ReviewMessage::Deleted => text.literal("삭제됨"),
        ReviewMessage::Modified => text.literal("수정됨"),
        ReviewMessage::Renamed => text.literal("이름 변경됨"),
        ReviewMessage::Copied => text.literal("복사됨"),
        ReviewMessage::Outdated => text.literal("오래됨"),
        ReviewMessage::Pending => text.literal("대기 중"),
        ReviewMessage::ResolvedThread { path, line, .. } => {
            text.literal("해결된 대화 · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("답글 달기…"),
        ReviewMessage::AddToReview => text.literal("리뷰에 추가"),
        ReviewMessage::ReopenConversation => text.literal("대화 다시 열기"),
        ReviewMessage::ResolveConversation => text.literal("대화 해결"),
        ReviewMessage::ReviewChanges => text.literal("변경 사항 리뷰"),
        ReviewMessage::PendingCount { count, .. } => {
            text.literal("대기 중 ");
            text.number(count);
            text.literal("개");
        }
        ReviewMessage::ReviewPlaceholder => text.literal("댓글을 남기세요(Markdown 지원)"),
        ReviewMessage::RequestChanges => text.literal("변경 요청"),
        ReviewMessage::Approve => text.literal("승인"),
        ReviewMessage::DiscardReview => text.literal("대기 중인 리뷰 취소"),
        ReviewMessage::DismissReview => text.literal("리뷰 기각"),
        ReviewMessage::DismissReason => text.literal("이 리뷰를 기각하는 이유는 무엇인가요?"),
        ReviewMessage::Dismiss => text.literal("기각"),
        ReviewMessage::DismissedBy { by, message, .. } => {
            text.argument(by);
            text.literal(" 님이 기각함: ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("리뷰어"),
        ReviewMessage::NoReviews => text.literal("아직 리뷰가 없습니다"),
        ReviewMessage::RemoveRequestFor { name, .. } => {
            text.argument(name);
            text.literal(" 님에 대한 리뷰 요청 삭제");
        }
        ReviewMessage::RemoveRequest => text.literal("요청 삭제"),
        ReviewMessage::RequestReviewFrom => text.literal("리뷰 요청 대상"),
        ReviewMessage::Username => text.literal("사용자 이름"),
        ReviewMessage::RequestReview => text.literal("리뷰 요청"),
        ReviewMessage::Approved => text.literal("승인됨"),
        ReviewMessage::ChangesRequested => text.literal("변경 요청됨"),
        ReviewMessage::Dismissed => text.literal("기각됨"),
        ReviewMessage::Commented => text.literal("댓글 남김"),
        ReviewMessage::AwaitingReview => text.literal("리뷰 대기 중"),
    }
}
