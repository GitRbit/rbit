//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;
use crate::catalog::shell::recovery::RecoverySentence;

entries! { ZhHans:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("重置密码"),
        RecoveryMessage::ForgotLede => text.literal("输入你账户的邮箱地址，我们会给你发送一个用于设置新密码的链接。"),
        RecoveryMessage::ForgotWithoutEmail => {
            text.literal("此服务器无法发送邮件，因此不能通过邮件重置密码。请联系管理员为你设置新密码。");
        }
        RecoveryMessage::CheckEmail => text.literal("请查收邮件"),
        RecoveryMessage::LinkSent => {
            text.literal("如果有账户使用该地址，我们已向其发送了一个用于设置新密码的链接。该链接只能使用一次，一小时后失效。");
        }
        RecoveryMessage::TryAgain => text.literal("重试"),
        RecoveryMessage::SendResetLink => text.literal("发送重置链接"),
        RecoveryMessage::RequestNewLink => text.literal("申请新链接"),
        RecoveryMessage::NewLinkWithoutEmail => {
            text.literal("此服务器无法发送邮件，因此不能发送新链接。请联系管理员为你设置新密码。");
        }
        RecoveryMessage::ChooseNewPassword => text.literal("设置新密码"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("账户 ");
            text.argument(username);
            text.literal("。今后你将使用新密码登录。");
        }
        RecoveryMessage::NewPassword => text.literal("新密码"),
        RecoveryMessage::ConfirmNewPassword => text.literal("确认新密码"),
        RecoveryMessage::ChangePassword => text.literal("修改密码"),
        RecoveryMessage::BackToSignIn => text.literal("返回登录"),
        RecoveryMessage::PasswordChanged => text.literal("密码已修改"),
        RecoveryMessage::PasswordChangedText => text.literal("新密码已设置。下次登录时请使用新密码。"),
        RecoveryMessage::LinkExpired => text.literal("此链接已过期"),
        RecoveryMessage::ResetLinkExpiredText => {
            text.literal("密码重置链接只能使用一次，一小时后失效。请申请新链接以继续。");
        }
        RecoveryMessage::AccountSuspended => text.literal("此账户已被停用"),
        RecoveryMessage::AccountSuspendedText => text.literal("无法重置它的密码。请联系管理员。"),
        RecoveryMessage::EmailVerified => text.literal("邮箱地址已验证"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("感谢你确认 ");
            text.argument(address);
            text.literal("。");
        }
        RecoveryMessage::VerifyLinkExpiredText => {
            text.literal("验证链接只能使用一次，两天后失效，且地址变更后即失效。请登录并在账户设置中重新发送。");
        }
        RecoveryMessage::GoToAccountSettings => text.literal("前往账户设置"),
        RecoveryMessage::SignIn => text.literal("登录"),
        RecoveryMessage::ConfirmEmailTitle => text.literal("确认你的邮箱地址"),
        RecoveryMessage::ConfirmEmailText(address) => {
            text.literal("确认 ");
            text.argument(address);
            text.literal(" 为你账户的邮箱地址。如果这不是你本人的操作，请关闭此页面。");
        }
        RecoveryMessage::ConfirmEmail => text.literal("确认邮箱地址"),
    }
    RecoverySentence => RichWriter |text| {
        RecoverySentence::NothingArrived => {
            text.literal("没有收到？请检查垃圾邮件文件夹，或");
            text.slot(0);
            text.literal("。");
        }
        RecoverySentence::RememberedSignIn => {
            text.literal("想起来了？");
            text.slot(0);
        }
    }
}
