//! Japanese (`ja`) entries for `catalog/theme_preview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme_preview::ThemePreviewMessage;

entries! { Ja:
    ThemePreviewMessage => MessageWriter |text| {
        ThemePreviewMessage::Star => text.literal("スター"),
        ThemePreviewMessage::Clone => text.literal("クローン"),
        ThemePreviewMessage::Code => text.literal("コード"),
        ThemePreviewMessage::Issues => text.literal("Issue"),
        ThemePreviewMessage::PullRequests => text.literal("プルリクエスト"),
        ThemePreviewMessage::Releases => text.literal("リリース"),
        ThemePreviewMessage::Lines { count } => {
            text.number(count);
            text.literal(" 行");
        }
        ThemePreviewMessage::ChecksPassed => text.literal("チェック成功"),
        ThemePreviewMessage::ReviewPending => text.literal("レビュー待ち"),
        ThemePreviewMessage::Failing { count } => {
            text.number(count);
            text.literal(" 件失敗");
        }
        ThemePreviewMessage::Tagline => text.literal("高速で静かなセルフホスト型 Git。"),
        ThemePreviewMessage::Link => text.literal("続きを読む"),
        ThemePreviewMessage::Updated => text.literal("2 時間前に更新"),
    }
}
