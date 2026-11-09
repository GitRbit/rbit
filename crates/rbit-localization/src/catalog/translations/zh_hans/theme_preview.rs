//! Simplified Chinese (`zh-Hans`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;

entries! { ZhHans:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("星标"),
        ThemePreviewMessage::Clone => text.literal("克隆"),
        ThemePreviewMessage::Code => text.literal("代码"),
        ThemePreviewMessage::Issues => text.literal("议题"),
        ThemePreviewMessage::PullRequests => text.literal("拉取请求"),
        ThemePreviewMessage::Releases => text.literal("发行版"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(" 行");
        }
        ThemePreviewMessage::ChecksPassed => text.literal("检查已通过"),
        ThemePreviewMessage::ReviewPending => text.literal("等待审查"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(" 个失败");
        }
        ThemePreviewMessage::Tagline => text.literal("自托管的 Git，快速而安静。"),
        ThemePreviewMessage::Link => text.literal("阅读更多"),
        ThemePreviewMessage::Updated => text.literal("2 小时前更新"),
    }
}
