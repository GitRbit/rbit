//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { ZhHans:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("更新你的密码"),
        AccountMessage::Title => text.literal("账户"),
        AccountMessage::Intro => text.literal("你的登录方式。修改时需要输入当前密码。"),
        AccountMessage::Username => text.literal("用户名"),
        AccountMessage::UsernameUpdated => text.literal("用户名已更新。"),
        AccountMessage::UsernameHint => text.literal(
            "在其他人使用你的旧用户名之前，使用旧用户名的个人资料链接和 Git 远程地址仍然有效。请更新你的链接和远程地址。\
             今后请使用新用户名或邮箱地址登录。",
        ),
        AccountMessage::CurrentPassword => text.literal("当前密码"),
        AccountMessage::ChangeUsername => text.literal("修改用户名"),
        AccountMessage::VerificationSent => text.literal("验证邮件已发送。请查收邮件。"),
        AccountMessage::Verified => text.literal("已验证"),
        AccountMessage::Unverified => text.literal("未验证"),
        AccountMessage::VerifiedText => text.literal("你已确认此地址属于你。"),
        AccountMessage::PendingText => text.literal("请通过我们发送给你的邮件中的链接确认此地址。"),
        AccountMessage::UnavailableText => text.literal("此服务器无法发送邮件，因此无法验证该地址。"),
        AccountMessage::Resend => text.literal("重新发送验证邮件"),
        AccountMessage::EmailAddress => text.literal("邮箱地址"),
        AccountMessage::EmailUpdated => text.literal("邮箱地址已更新。"),
        AccountMessage::Email => text.literal("邮箱"),
        AccountMessage::EmailHintLead => text.literal("你可以使用此地址或用户名登录。当前地址："),
        AccountMessage::EmailHintTail => text.literal("。"),
        AccountMessage::UpdateEmail => text.literal("更新邮箱地址"),
        AccountMessage::Password => text.literal("密码"),
        AccountMessage::PasswordChanged => text.literal("密码已修改。"),
        AccountMessage::NewPassword => text.literal("新密码"),
        AccountMessage::ConfirmPassword => text.literal("确认新密码"),
        AccountMessage::ChangePassword => text.literal("修改密码"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal("管理员使用初始密码创建了你的账户。在继续使用 ");
            text.argument(site);
            text.literal(" 之前，请设置一个新密码。");
        }
        AccountText::PasswordHint(length) => {
            text.literal("至少 ");
            text.number(length);
            text.literal(" 个字符。几个互不相关的词语组合起来，就是一个既安全又好记的密码。");
        }
    }
}
