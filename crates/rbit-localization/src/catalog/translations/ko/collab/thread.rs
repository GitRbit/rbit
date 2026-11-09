//! Korean (`ko`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("열림"),
        ThreadMessage::Closed => text.literal("닫힘"),
        ThreadMessage::Merged => text.literal("병합됨"),
        ThreadMessage::Draft => text.literal("초안"),
        ThreadMessage::Title => text.literal("제목"),
        ThreadMessage::Description => text.literal("설명"),
        ThreadMessage::Edit => text.literal("편집"),
        ThreadMessage::Save => text.literal("저장"),
        ThreadMessage::Cancel => text.literal("취소"),
        ThreadMessage::Delete => text.literal("삭제"),
        ThreadMessage::Optional => text.literal("선택 사항"),
        ThreadMessage::MarkdownSupported => text.literal("Markdown이 지원됩니다"),
        ThreadMessage::SignIn => text.literal("로그인"),
        ThreadMessage::LoadMore => text.literal("더 보기"),
        ThreadMessage::AddComment => text.literal("댓글 추가"),
        ThreadMessage::CommentPlaceholder => text.literal("댓글을 작성하세요. 사용자는 @, 이슈나 풀 리퀘스트는 #으로 언급할 수 있습니다."),
        ThreadMessage::MentionSuggestions => text.literal("멘션 추천"),
        ThreadMessage::CommentHint => text.literal("Markdown이 지원됩니다. @name, #123 또는 owner/repo#123으로 사용자, 이슈, 풀 리퀘스트를 링크할 수 있습니다."),
        ThreadMessage::LockedHint => text.literal("이 대화는 잠겨 있어 공동 작업자만 댓글을 달 수 있습니다."),
        ThreadMessage::CloseIssue => text.literal("이슈 닫기"),
        ThreadMessage::ClosePull => text.literal("풀 리퀘스트 닫기"),
        ThreadMessage::CloseNotPlanned => text.literal("계획 없음으로 닫기"),
        ThreadMessage::ReopenIssue => text.literal("이슈 다시 열기"),
        ThreadMessage::ReopenPull => text.literal("풀 리퀘스트 다시 열기"),
        ThreadMessage::Comment => text.literal("댓글"),
        ThreadMessage::LockConversation => text.literal("대화 잠그기"),
        ThreadMessage::UnlockConversation => text.literal("대화 잠금 해제"),
        ThreadMessage::Bot => text.literal("봇"),
        ThreadMessage::Edited => text.literal("편집됨"),
        ThreadMessage::Author => text.literal("작성자"),
        ThreadMessage::Reply => text.literal("답글"),
        ThreadMessage::DeleteCommentConfirm => text.literal("이 댓글을 삭제할까요?"),
        ThreadMessage::DeleteComment => text.literal("댓글 삭제"),
        ThreadMessage::NoDescription => text.literal("설명이 없습니다."),
    }
}
