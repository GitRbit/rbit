//! Korean (`ko`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};

entries! { Ko:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle => text.literal("로그인"),
        AuthMessage::SignUpTitle => text.literal("가입"),
        AuthMessage::SignInTo(site) => {
            text.argument(site);
            text.literal("에 로그인");
        }
        AuthMessage::SignInLede => text.literal("사용자 이름이나 이메일 주소를 입력하세요."),
        AuthMessage::UsernameOrEmail => text.literal("사용자 이름 또는 이메일"),
        AuthMessage::Password => text.literal("비밀번호"),
        AuthMessage::ForgotPassword => text.literal("비밀번호를 잊으셨나요?"),
        AuthMessage::SignIn => text.literal("로그인"),
        AuthMessage::Or => text.literal("또는"),
        AuthMessage::SignInWith(provider) => {
            text.argument(provider);
            text.literal(" 계정으로 로그인");
        }
        AuthMessage::NoAccount => text.literal("계정이 없으신가요?"),
        AuthMessage::SignUp => text.literal("가입"),
        AuthMessage::CreateYourAccount => text.literal("계정 만들기"),
        AuthMessage::HostYourCode(site) => {
            text.argument(site);
            text.literal("에서 코드를 호스팅하세요.");
        }
        AuthMessage::Username => text.literal("사용자 이름"),
        AuthMessage::UsernameHint => text.literal("영문자, 숫자, '-', '_', '.'"),
        AuthMessage::Email => text.literal("이메일"),
        AuthMessage::MinimumLength(count) => {
            text.literal("최소 ");
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal("자.");
        }
        AuthMessage::SetupToken => text.literal("설정 토큰"),
        AuthMessage::SetupTokenHint => text.literal("첫 번째 계정이 서버를 관리합니다. 토큰은 서버 로그에서 확인하세요."),
        AuthMessage::CreateAccount => text.literal("계정 만들기"),
        AuthMessage::RegistrationClosed => text.literal("이 서버는 가입을 받지 않습니다. 관리자에게 계정을 요청하세요."),
        AuthMessage::HaveAccount => text.literal("이미 계정이 있으신가요?"),
        AuthMessage::TwoFactorTitle => text.literal("2단계 인증"),
        AuthMessage::PasskeyOrApp => text.literal("패스키를 사용하거나 인증 앱의 6자리 코드를 입력하세요."),
        AuthMessage::PasskeyOnly => text.literal("패스키나 보안 키를 사용하여 로그인을 완료하세요."),
        AuthMessage::AppOnly => text.literal("인증 앱의 6자리 코드를 입력하세요."),
        AuthMessage::UsePasskey => text.literal("패스키 사용"),
        AuthMessage::PasskeyUnsupported => text.literal("이 브라우저에서는 패스키를 사용할 수 없습니다. 대신 코드를 입력하세요."),
        AuthMessage::AuthenticationCode => text.literal("인증 코드"),
        AuthMessage::LostDevice => text.literal("기기를 분실하셨나요? 대신 복구 코드 중 하나를 입력하세요."),
        AuthMessage::RecoveryCode => text.literal("복구 코드"),
        AuthMessage::LostPasskey => text.literal("패스키를 분실하셨나요? 복구 코드 중 하나를 입력하세요."),
        AuthMessage::VerifyCode => text.literal("코드 확인"),
        AuthMessage::NotYou => text.literal("본인이 아니신가요?"),
        AuthMessage::DifferentAccount => text.literal("다른 계정으로 로그인"),
        AuthMessage::ProviderLede(provider, site) => {
            text.argument(provider);
            text.literal(" 계정으로 로그인했습니다. ");
            text.argument(site);
            text.literal("에서 사용할 사용자 이름을 선택하세요.");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.argument(provider);
            text.literal("에서 확인됨. 다른 주소는 이메일로 확인해야 합니다.");
        }
        AuthMessage::ConfirmByEmail => text.literal("이 주소를 확인하는 이메일이 발송됩니다."),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.argument(provider);
            text.literal(" 계정으로 로그인하게 됩니다. 재설정 링크로 비밀번호를 설정하기 전까지는 계정에 비밀번호가 없습니다. HTTPS를 통해 Git을 사용하려면 개인 액세스 토큰이 필요합니다.");
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("후 보안 설정에서 ");
            text.argument(provider);
            text.literal(" 계정을 연결하세요.");
        }
        AuthMessage::SignInFailed => text.literal("로그인하지 못했습니다"),
        AuthMessage::BackToSignIn => text.literal("로그인으로 돌아가기"),
        AuthMessage::BackToSecuritySettings => text.literal("보안 설정으로 돌아가기"),
        AuthMessage::IncorrectCredentials => text.literal("사용자 이름 또는 비밀번호가 올바르지 않습니다."),
        AuthMessage::Suspended => text.literal("이 계정은 정지되었습니다. 관리자에게 문의하세요."),
        AuthMessage::SignInExpired => text.literal("로그인이 만료되었습니다. 다시 로그인하세요."),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "로그인 시도가 너무 많습니다. ",
                Attempt::SignUp => "가입 시도가 너무 많습니다. ",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal("초 후에 다시 시도하세요.");
            } else {
                text.number(seconds.div_ceil(60));
                text.literal("분 후에 다시 시도하세요.");
            }
        }
    }
}
