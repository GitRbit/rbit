//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { ZhHans:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("安全"),
        SecurityMessage::Intro => text.literal("使用通行密钥或身份验证器应用保护你的账户，并随时了解你在哪些地方登录。"),
        SecurityMessage::TwoFactor => text.literal("双重身份验证"),
        SecurityMessage::On => text.literal("已开启"),
        SecurityMessage::Off => text.literal("已关闭"),
        SecurityMessage::EnabledLead => {
            text.literal("登录时，在输入密码后还需要通行密钥或验证码。通过 HTTPS 使用 Git 需要");
        }
        SecurityMessage::TokenLink => text.literal("个人访问令牌"),
        SecurityMessage::EnabledTail => text.literal("；你的密码在那里不再有效。"),
        SecurityMessage::Disabled => text.literal("为登录添加第二步验证：通行密钥、身份验证器应用，或两者兼用。"),
        SecurityMessage::Passkeys => text.literal("通行密钥和安全密钥"),
        SecurityMessage::PasskeysText => {
            text.literal("使用指纹、面容、设备 PIN、手机或 YubiKey 等硬件密钥确认登录。");
        }
        SecurityMessage::Remove => text.literal("移除"),
        SecurityMessage::OnlyFactor => {
            text.literal("这是你唯一的第二因素：移除它会关闭双重身份验证，并删除你的恢复码。");
        }
        SecurityMessage::PasskeyGone => text.literal("你将无法再使用此通行密钥登录。"),
        SecurityMessage::Name => text.literal("名称"),
        SecurityMessage::PasskeyPlaceholder => text.literal("例如：MacBook 或 YubiKey"),
        SecurityMessage::AddPasskey => text.literal("添加通行密钥"),
        SecurityMessage::PasskeyUnsupported => text.literal("添加通行密钥需要支持通行密钥并已启用 JavaScript 的浏览器。"),
        SecurityMessage::PasskeyUrlLead => text.literal("使用通行密钥需要将"),
        SecurityMessage::PasskeyUrlTail => text.literal("设置为此站点的地址。"),
        SecurityMessage::Authenticator => text.literal("身份验证器应用"),
        SecurityMessage::AuthenticatorText => {
            text.literal("使用 1Password、Aegis 或 Google Authenticator 等应用生成的 6 位验证码。");
        }
        SecurityMessage::SetUp => text.literal("设置"),
        SecurityMessage::RemoveAuthenticator => text.literal("移除身份验证器应用"),
        SecurityMessage::PasswordAlone => text.literal("你的账户将仅受密码保护，你的恢复码也会被删除。"),
        SecurityMessage::PasskeysStay => text.literal("你的通行密钥仍将作为第二因素。"),
        SecurityMessage::RecoveryCodes => text.literal("恢复码"),
        SecurityMessage::Regenerate => text.literal("生成新的恢复码"),
        SecurityMessage::CodesStop => text.literal("你当前的恢复码将失效。"),
        SecurityMessage::GenerateCodes => text.literal("生成新恢复码"),
        SecurityMessage::SignInMethods => text.literal("登录方式"),
        SecurityMessage::Password => text.literal("密码"),
        SecurityMessage::NoPassword => text.literal("没有密码"),
        SecurityMessage::WithPassword => text.literal("使用你的密码或已关联的账户登录。"),
        SecurityMessage::LinkedOnlyLead => {
            text.literal("你的账户没有密码：你通过已关联的账户登录。如需设置密码，请申请");
        }
        SecurityMessage::ResetLink => text.literal("密码重置链接"),
        SecurityMessage::LinkedOnlyMiddle => text.literal("。通过 HTTPS 使用 Git 需要"),
        SecurityMessage::LinkedOnlyTail => text.literal("。"),
        SecurityMessage::NoLongerAvailable => text.literal("已不可用"),
        SecurityMessage::Unlink => text.literal("取消关联"),
        SecurityMessage::OnlyWay => text.literal("唯一的登录方式"),
        SecurityMessage::NotLinked => text.literal("未关联"),
        SecurityMessage::Link => text.literal("关联"),
        SecurityMessage::Sessions => text.literal("会话"),
        SecurityMessage::SignOutOthers => text.literal("退出所有其他会话"),
        SecurityMessage::ThisDevice => text.literal("此设备"),
        SecurityMessage::SignOut => text.literal("退出登录"),
        SecurityMessage::RecentActivity => text.literal("最近活动"),
        SecurityMessage::NoActivity => text.literal("还没有活动"),
        SecurityMessage::NoActivityText => text.literal("你账户的登录和安全变更会显示在这里。"),
        SecurityMessage::SetupTitle => text.literal("设置身份验证器应用"),
        SecurityMessage::SetupIntro => text.literal("用身份验证器应用扫描二维码，然后输入应用显示的 6 位验证码。"),
        SecurityMessage::CantScan => text.literal("无法扫描？请改为输入此密钥"),
        SecurityMessage::CopyKey => text.literal("复制密钥"),
        SecurityMessage::Copy => text.literal("复制"),
        SecurityMessage::TimeBased => text.literal("基于时间，6 位数字，每 30 秒更新一次。"),
        SecurityMessage::ConfirmCode => text.literal("2. 确认验证码"),
        SecurityMessage::AuthenticationCode => text.literal("验证码"),
        SecurityMessage::Cancel => text.literal("取消"),
        SecurityMessage::Enable => text.literal("启用"),
        SecurityMessage::TwoFactorOn => text.literal("双重身份验证已开启。"),
        SecurityMessage::SaveCodes => text.literal("请立即保存这些恢复码"),
        SecurityMessage::CodesText => text.literal(
            "如果你丢失了通行密钥或身份验证器应用，每个恢复码可用于登录一次。它们不会再次显示，\
             请将其保存在安全的地方，例如密码管理器中。",
        ),
        SecurityMessage::SavedCodes => text.literal("我已保存恢复码"),
        SecurityMessage::AppOrRecoveryCode => text.literal("验证码或恢复码"),
        SecurityMessage::RecoveryCode => text.literal("恢复码"),
        SecurityMessage::PasskeyInstead => text.literal("也可以使用你的某个通行密钥确认，而不输入验证码。"),
        SecurityMessage::AuthenticatorRemoved => text.literal("身份验证器应用已移除。"),
        SecurityMessage::AuthenticatorAdded => text.literal("身份验证器应用已添加。"),
        SecurityMessage::PasskeyAdded => text.literal("通行密钥已添加。"),
        SecurityMessage::PasskeyRemoved => text.literal("通行密钥已移除。"),
        SecurityMessage::SessionRevoked => text.literal("该会话已退出登录。"),
        SecurityMessage::OtherSessionsRevoked => text.literal("已退出所有其他会话。"),
        SecurityMessage::SetupCancelled => text.literal("已取消双重身份验证设置。"),
        SecurityMessage::IdentityLinked => text.literal("账户已关联。你现在可以用它登录了。"),
        SecurityMessage::IdentityUnlinked => text.literal("账户已取消关联。"),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. 将 ");
            text.argument(site);
            text.literal(" 添加到你的应用");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.literal("取消关联 ");
            text.argument(provider);
        }
        SecurityText::SignOutLabel(device) => {
            text.literal("退出 ");
            text.argument(device);
            text.literal(" 上的登录");
        }
        SecurityText::WithPasskey(action) => {
            text.literal("使用通行密钥");
            text.argument(action);
        }
    }
}
