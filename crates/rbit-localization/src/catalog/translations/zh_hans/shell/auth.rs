//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};

entries! { ZhHans:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle | AuthMessage::SignIn => text.literal("登录"),
        AuthMessage::SignUpTitle | AuthMessage::SignUp => text.literal("注册"),
        AuthMessage::SignInTo(site) => {
            text.literal("登录 ");
            text.argument(site);
        }
        AuthMessage::SignInLede => text.literal("使用你的用户名或邮箱地址。"),
        AuthMessage::UsernameOrEmail => text.literal("用户名或邮箱地址"),
        AuthMessage::Password => text.literal("密码"),
        AuthMessage::ForgotPassword => text.literal("忘记密码？"),
        AuthMessage::Or => text.literal("或"),
        AuthMessage::SignInWith(provider) => {
            text.literal("使用 ");
            text.argument(provider);
            text.literal(" 登录");
        }
        AuthMessage::NoAccount => text.literal("还没有账户？"),
        AuthMessage::CreateYourAccount => text.literal("创建你的账户"),
        AuthMessage::HostYourCode(site) => {
            text.literal("在 ");
            text.argument(site);
            text.literal(" 上托管你的代码。");
        }
        AuthMessage::Username => text.literal("用户名"),
        AuthMessage::UsernameHint => text.literal("字母、数字、“-”、“_”和“.”"),
        AuthMessage::Email => text.literal("邮箱地址"),
        AuthMessage::MinimumLength(count) => {
            text.literal("至少 ");
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" 个字符。");
        }
        AuthMessage::SetupToken => text.literal("初始化令牌"),
        AuthMessage::SetupTokenHint => text.literal("第一个账户将管理此服务器。令牌可在服务器日志中找到。"),
        AuthMessage::CreateAccount => text.literal("创建账户"),
        AuthMessage::RegistrationClosed => text.literal("此服务器已关闭注册。请向管理员申请账户。"),
        AuthMessage::HaveAccount => text.literal("已有账户？"),
        AuthMessage::TwoFactorTitle => text.literal("双重身份验证"),
        AuthMessage::PasskeyOrApp => text.literal("使用你的通行密钥，或输入身份验证器应用中的 6 位验证码。"),
        AuthMessage::PasskeyOnly => text.literal("使用你的通行密钥或安全密钥完成登录。"),
        AuthMessage::AppOnly => text.literal("输入身份验证器应用中的 6 位验证码。"),
        AuthMessage::UsePasskey => text.literal("使用通行密钥"),
        AuthMessage::PasskeyUnsupported => text.literal("此浏览器无法使用通行密钥。请改为输入验证码。"),
        AuthMessage::AuthenticationCode => text.literal("验证码"),
        AuthMessage::LostDevice => text.literal("设备丢失了？请改为输入一个恢复码。"),
        AuthMessage::RecoveryCode => text.literal("恢复码"),
        AuthMessage::LostPasskey => text.literal("通行密钥丢失了？请输入一个恢复码。"),
        AuthMessage::VerifyCode => text.literal("验证"),
        AuthMessage::NotYou => text.literal("不是你？"),
        AuthMessage::DifferentAccount => text.literal("使用其他账户登录"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("你已通过 ");
            text.argument(provider);
            text.literal(" 登录。请为 ");
            text.argument(site);
            text.literal(" 选择一个用户名。");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("已由 ");
            text.argument(provider);
            text.literal(" 验证。使用其他地址需要通过邮件确认。");
        }
        AuthMessage::ConfirmByEmail => text.literal("你将收到一封邮件，用于确认此地址。"),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("你将通过 ");
            text.argument(provider);
            text.literal(" 登录；在你通过重置链接设置密码之前，账户没有密码。通过 HTTPS 使用 Git 需要个人访问令牌。");
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("，然后在安全设置中关联 ");
            text.argument(provider);
            text.literal("。");
        }
        AuthMessage::SignInFailed => text.literal("登录失败"),
        AuthMessage::BackToSignIn => text.literal("返回登录"),
        AuthMessage::BackToSecuritySettings => text.literal("返回安全设置"),
        AuthMessage::IncorrectCredentials => text.literal("用户名或密码不正确。"),
        AuthMessage::Suspended => text.literal("此账户已被停用。请联系管理员。"),
        AuthMessage::SignInExpired => text.literal("登录已过期。请重新登录。"),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "登录尝试次数过多。请在 ",
                Attempt::SignUp => "注册尝试次数过多。请在 ",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal(" 秒后重试。");
            } else {
                text.number(seconds.div_ceil(60));
                text.literal(" 分钟后重试。");
            }
        }
    }
}
