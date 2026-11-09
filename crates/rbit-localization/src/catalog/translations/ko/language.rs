//! Korean (`ko`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { Ko:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("표시 언어"),
        LanguageMessage::Label => text.literal("선호 언어"),
        LanguageMessage::BrowserDefault => text.literal("브라우저 / 기본값"),
        LanguageMessage::BrowserHelp => text.literal("브라우저에서 지원되는 언어를 따르며, 사용할 수 없으면 영어를 사용합니다."),
        LanguageMessage::Coverage => text.literal("아직 사용자의 언어로 번역되지 않은 텍스트는 영어로 표시됩니다."),
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("언어 저장"),
        LanguageMessage::Saved => text.literal("언어 설정을 저장했습니다."),
        LanguageMessage::Invalid => text.literal("목록에 있는 언어 또는 브라우저 기본값을 선택하세요."),
        LanguageMessage::Close => text.literal("닫기"),
        LanguageMessage::DeviceOnly => text.literal("이 브라우저는 언어만 저장하는 쿠키로 선택을 기억합니다."),
    }
}
