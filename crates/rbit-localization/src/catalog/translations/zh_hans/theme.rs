//! Simplified Chinese (`zh-Hans`) entries for `catalog/theme.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::theme::ThemeMessage;
use crate::catalog::theme::ThemeSentence;

entries! { ZhHans:
    ThemeMessage => MessageWriter |text| {
        ThemeMessage::Choose => text.literal("选择主题"),
        ThemeMessage::Search => text.literal("搜索主题"),
        ThemeMessage::Close => text.literal("关闭"),
        ThemeMessage::Show => text.literal("显示"),
        ThemeMessage::All => text.literal("全部"),
        ThemeMessage::Dark => text.literal("深色"),
        ThemeMessage::Light => text.literal("浅色"),
        ThemeMessage::Colorblind => text.literal("色盲友好"),
        ThemeMessage::Families => text.literal("主题系列"),
        ThemeMessage::Recent => text.literal("最近使用"),
        ThemeMessage::Themes => text.literal("主题"),
        ThemeMessage::Empty => text.literal("没有与搜索匹配的主题。"),
        ThemeMessage::Preview => text.literal("预览"),
        ThemeMessage::System => text.literal("跟随系统"),
        ThemeMessage::BuiltIn => text.literal("内置"),
        ThemeMessage::More => text.literal("更多主题"),
        ThemeMessage::Auto => text.literal("自动"),
        ThemeMessage::Apply => text.literal("应用"),
        ThemeMessage::Applied => text.literal("已应用"),
        ThemeMessage::By => text.literal("作者"),
        ThemeMessage::Credit => text.literal("rbit 内置"),
        ThemeMessage::OrApply => text.literal("或点击“应用”"),
    }
    ThemeSentence => RichWriter |text| {
        ThemeSentence::Count { .. } => {
            text.slot(0);
            text.literal(" 个主题");
        }
    }
}
