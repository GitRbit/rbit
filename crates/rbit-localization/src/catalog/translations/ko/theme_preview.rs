//! Korean (`ko`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;

entries! { Ko:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("스타"),
        ThemePreviewMessage::Clone => text.literal("클론"),
        ThemePreviewMessage::Code => text.literal("코드"),
        ThemePreviewMessage::Issues => text.literal("이슈"),
        ThemePreviewMessage::PullRequests => text.literal("풀 리퀘스트"),
        ThemePreviewMessage::Releases => text.literal("릴리스"),
        ThemePreviewMessage::Lines { count, .. } => {
            text.number(count);
            text.literal("줄");
        }
        ThemePreviewMessage::ChecksPassed => text.literal("검사 통과"),
        ThemePreviewMessage::ReviewPending => text.literal("리뷰 대기 중"),
        ThemePreviewMessage::Failing { count, .. } => {
            text.number(count);
            text.literal("개 실패");
        }
        ThemePreviewMessage::Tagline => text.literal("빠르고 조용한 자체 호스팅 Git."),
        ThemePreviewMessage::Link => text.literal("자세히 보기"),
        ThemePreviewMessage::Updated => text.literal("2시간 전에 업데이트됨"),
    }
}
