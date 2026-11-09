//! Simplified Chinese (`zh-Hans`) entries for `catalog/language.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::language::LanguageMessage;

entries! { ZhHans:
    LanguageMessage => MessageWriter |text| {
        LanguageMessage::Title => text.literal("显示语言"),
        LanguageMessage::Label => text.literal("首选语言"),
        LanguageMessage::BrowserDefault => text.literal("浏览器 / 默认"),
        LanguageMessage::BrowserHelp => text.literal("跟随浏览器中受支持的语言；不可用时使用英语。"),
        LanguageMessage::Coverage => text.literal("尚未翻译成你的语言的文本会以英语显示。"),
        LanguageMessage::English => text.literal("English"),
        LanguageMessage::Spanish => text.literal("Español"),
        LanguageMessage::Save => text.literal("保存语言"),
        LanguageMessage::Saved => text.literal("你的语言偏好已保存。"),
        LanguageMessage::Invalid => text.literal("请选择列表中的一种语言或浏览器默认设置。"),
        LanguageMessage::Close => text.literal("关闭"),
        LanguageMessage::DeviceOnly => text.literal("此浏览器会用一个只保存语言的 Cookie 记住你的选择。"),
    }
}
