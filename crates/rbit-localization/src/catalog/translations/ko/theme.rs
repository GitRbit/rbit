//! Korean (`ko`) entries for `catalog/theme.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;

entries! { Ko:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("테마 선택"),
        ThemeMessage::Search => text.literal("테마 검색"),
        ThemeMessage::Close => text.literal("닫기"),
        ThemeMessage::Show => text.literal("표시"),
        ThemeMessage::All => text.literal("전체"),
        ThemeMessage::Dark => text.literal("어두운 테마"),
        ThemeMessage::Light => text.literal("밝은 테마"),
        ThemeMessage::Colorblind => text.literal("색각 이상 지원"),
        ThemeMessage::Families => text.literal("테마 계열"),
        ThemeMessage::Recent => text.literal("최근 사용"),
        ThemeMessage::Themes => text.literal("테마"),
        ThemeMessage::Empty => text.literal("검색과 일치하는 테마가 없습니다."),
        ThemeMessage::Preview => text.literal("미리 보기"),
        ThemeMessage::System => text.literal("시스템"),
        ThemeMessage::BuiltIn => text.literal("기본 제공"),
        ThemeMessage::More => text.literal("테마 더 보기"),
        ThemeMessage::Auto => text.literal("자동"),
        ThemeMessage::Apply => text.literal("적용"),
        ThemeMessage::Applied => text.literal("적용됨"),
        ThemeMessage::By => text.literal("제작:"),
        ThemeMessage::Credit => text.literal("rbit 기본 제공"),
        ThemeMessage::ThemeSingular => text.literal("테마"),
        ThemeMessage::ThemePlural => text.literal("테마"),
        ThemeMessage::SearchPrefix => text.literal("검색:"),
        ThemeMessage::OrApply => text.literal("또는 적용 클릭"),
    }
}
