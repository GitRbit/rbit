//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/profile.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { ZhHans:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("个人资料"),
        ProfileMessage::IntroLead => text.literal("显示在"),
        ProfileMessage::IntroLink => text.literal("你的个人资料页面"),
        ProfileMessage::IntroTail => text.literal("以及你的动态旁边。这里的所有内容都是公开且可选的。"),
        ProfileMessage::Updated => text.literal("个人资料已更新。"),
        ProfileMessage::DisplayName => text.literal("显示名称"),
        ProfileMessage::DisplayNameHint => text.literal("在空间允许时代替用户名显示。"),
        ProfileMessage::Bio => text.literal("个人简介"),
        ProfileMessage::BioHint => text.literal("最多 300 个字符。"),
        ProfileMessage::Website => text.literal("网站"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("所在地"),
        ProfileMessage::Save => text.literal("保存个人资料"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("外观"),
        AppearanceMessage::Intro => text.literal(
            "你的主题保存在账户中，会在你登录的每台设备上生效。将鼠标悬停在主题上可预览，单击即可选用。",
        ),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("通知"),
        NotificationsMessage::IntroLead => text.literal("所有通知仍会显示在"),
        NotificationsMessage::IntroLink => text.literal("你的收件箱"),
        NotificationsMessage::IntroTail => text.literal("中。请选择还要发送到你邮箱地址的通知。"),
        NotificationsMessage::Saved => text.literal("偏好设置已保存。"),
        NotificationsMessage::Unavailable => text.literal("此服务器无法发送邮件，因此通知只会显示在收件箱中。"),
        NotificationsMessage::VerifyLead => text.literal("要接收通知邮件，请验证"),
        NotificationsMessage::VerifyMiddle => text.literal("，可在"),
        NotificationsMessage::VerifyLink => text.literal("账户设置"),
        NotificationsMessage::VerifyTail => text.literal("中操作。"),
        NotificationsMessage::Email => text.literal("邮件"),
        NotificationsMessage::SendTo => text.literal("发送至"),
        NotificationsMessage::When => text.literal("发送时机"),
        NotificationsMessage::DigestTime => text.literal("摘要发送时间"),
        NotificationsMessage::DigestHint => text.literal(
            "摘要会略去你已在这里读过的讨论。每封邮件都附有一个链接，单击即可关闭邮件通知。",
        ),
        NotificationsMessage::Save => text.literal("保存偏好设置"),
        NotificationsMessage::Off => text.literal("关闭"),
        NotificationsMessage::OffText => text.literal("通知只显示在你的收件箱中。"),
        NotificationsMessage::Participating => text.literal("参与的"),
        NotificationsMessage::ParticipatingText => text.literal(
            "你创建、评论或审查过的讨论，以及你被提及、被分配或被请求审查时。",
        ),
        NotificationsMessage::Everything => text.literal("全部"),
        NotificationsMessage::EverythingText => text.literal("还包括你订阅但未参与的讨论。"),
        NotificationsMessage::Immediately => text.literal("立即"),
        NotificationsMessage::ImmediatelyText => text.literal("每条通知发生时各发送一封邮件。"),
        NotificationsMessage::Daily => text.literal("每日摘要"),
        NotificationsMessage::DailyText => text.literal("每天一封邮件，包含自上一封以来的所有通知。"),
        NotificationsMessage::Weekly => text.literal("每周摘要"),
        NotificationsMessage::WeeklyText => text.literal("每周一发送一封邮件，包含上一周的所有通知。"),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("默认（");
            text.argument(hour);
            text.literal("）");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("显示"),
        GalleryMessage::Search => text.literal("搜索主题"),
        GalleryMessage::BuiltIn => text.literal("内置"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("没有符合搜索条件的主题。"),
        GalleryMessage::CreditLead => text.literal("着色主题由"),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::CreditTail => text.literal("的 base16 和 base24 配色方案（MIT）生成；将鼠标悬停在主题上可查看其作者。"),
        GalleryMessage::Licence => text.literal("许可证和致谢"),
        GalleryMessage::LicenceTail => text.literal("。"),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("搜索 ");
            text.number(total);
            text.literal(" 个主题");
        }
    }
}
