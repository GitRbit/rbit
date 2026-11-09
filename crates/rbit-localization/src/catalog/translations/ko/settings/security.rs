//! Korean (`ko`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Ko:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("보안"),
        SecurityMessage::Intro => text.literal("패스키나 인증 앱으로 계정을 보호하고, 어디에서 로그인되어 있는지 확인하세요."),
        SecurityMessage::TwoFactor => text.literal("2단계 인증"),
        SecurityMessage::On => text.literal("켜짐"),
        SecurityMessage::Off => text.literal("꺼짐"),
        SecurityMessage::EnabledLead => text.literal("로그인할 때 비밀번호 다음에 패스키나 코드를 요구합니다. HTTPS로 Git을 사용하려면"),
        SecurityMessage::TokenLink => text.literal("개인 액세스 토큰"),
        SecurityMessage::EnabledTail => text.literal("이 필요하며, 그곳에서는 더 이상 비밀번호를 사용할 수 없습니다."),
        SecurityMessage::Disabled => text.literal("로그인에 두 번째 단계를 추가하세요: 패스키, 인증 앱 또는 둘 다."),
        SecurityMessage::Passkeys => text.literal("패스키 및 보안 키"),
        SecurityMessage::PasskeysText => text.literal("지문, 얼굴, 기기 PIN, 휴대폰 또는 YubiKey 같은 하드웨어 키로 로그인을 확인합니다."),
        SecurityMessage::Remove => text.literal("제거"),
        SecurityMessage::OnlyFactor => text.literal("유일한 두 번째 인증 수단입니다. 제거하면 2단계 인증이 꺼지고 복구 코드가 삭제됩니다."),
        SecurityMessage::PasskeyGone => text.literal("더 이상 이 패스키로 로그인할 수 없습니다."),
        SecurityMessage::Name => text.literal("이름"),
        SecurityMessage::PasskeyPlaceholder => text.literal("예: MacBook 또는 YubiKey"),
        SecurityMessage::AddPasskey => text.literal("패스키 추가"),
        SecurityMessage::PasskeyUnsupported => text.literal("패스키를 추가하려면 패스키를 지원하고 JavaScript가 켜진 브라우저가 필요합니다."),
        SecurityMessage::PasskeyUrlLead => text.literal("패스키를 사용하려면"),
        SecurityMessage::PasskeyUrlTail => text.literal("값을 이 사이트의 주소로 설정해야 합니다."),
        SecurityMessage::Authenticator => text.literal("인증 앱"),
        SecurityMessage::AuthenticatorText => text.literal("1Password, Aegis, Google Authenticator 같은 앱의 6자리 코드를 사용합니다."),
        SecurityMessage::SetUp => text.literal("설정"),
        SecurityMessage::RemoveAuthenticator => text.literal("인증 앱 제거"),
        SecurityMessage::PasswordAlone => text.literal("계정이 비밀번호로만 보호되며, 복구 코드가 삭제됩니다."),
        SecurityMessage::PasskeysStay => text.literal("패스키는 두 번째 인증 수단으로 계속 사용됩니다."),
        SecurityMessage::RecoveryCodes => text.literal("복구 코드"),
        SecurityMessage::Regenerate => text.literal("새 복구 코드 생성"),
        SecurityMessage::CodesStop => text.literal("현재 복구 코드는 더 이상 작동하지 않습니다."),
        SecurityMessage::GenerateCodes => text.literal("새 코드 생성"),
        SecurityMessage::SignInMethods => text.literal("로그인 방법"),
        SecurityMessage::Password => text.literal("비밀번호"),
        SecurityMessage::NoPassword => text.literal("비밀번호 없음"),
        SecurityMessage::WithPassword => text.literal("비밀번호나 연결된 계정으로 로그인합니다."),
        SecurityMessage::LinkedOnlyLead => text.literal("계정에 비밀번호가 없어 연결된 계정으로 로그인합니다. 비밀번호를 설정하려면"),
        SecurityMessage::ResetLink => text.literal("비밀번호 재설정 링크"),
        SecurityMessage::LinkedOnlyMiddle => text.literal("를 요청하세요. HTTPS로 Git을 사용하려면"),
        SecurityMessage::LinkedOnlyTail => text.literal("이 필요합니다."),
        SecurityMessage::NoLongerAvailable => text.literal("더 이상 사용할 수 없음"),
        SecurityMessage::Unlink => text.literal("연결 해제"),
        SecurityMessage::OnlyWay => text.literal("유일한 로그인 방법"),
        SecurityMessage::NotLinked => text.literal("연결되지 않음"),
        SecurityMessage::Link => text.literal("연결"),
        SecurityMessage::Sessions => text.literal("세션"),
        SecurityMessage::SignOutOthers => text.literal("다른 모든 곳에서 로그아웃"),
        SecurityMessage::ThisDevice => text.literal("이 기기"),
        SecurityMessage::SignOut => text.literal("로그아웃"),
        SecurityMessage::RecentActivity => text.literal("최근 활동"),
        SecurityMessage::NoActivity => text.literal("아직 활동이 없습니다"),
        SecurityMessage::NoActivityText => text.literal("계정의 로그인 및 보안 변경 내역이 여기에 표시됩니다."),
        SecurityMessage::SetupTitle => text.literal("인증 앱 설정"),
        SecurityMessage::SetupIntro => text.literal("인증 앱으로 QR 코드를 스캔한 다음 앱에 표시된 6자리 코드를 입력하세요."),
        SecurityMessage::CantScan => text.literal("스캔할 수 없나요? 대신 이 키를 입력하세요"),
        SecurityMessage::CopyKey => text.literal("키 복사"),
        SecurityMessage::Copy => text.literal("복사"),
        SecurityMessage::TimeBased => text.literal("시간 기반, 6자리, 30초마다 변경."),
        SecurityMessage::ConfirmCode => text.literal("2. 코드 확인"),
        SecurityMessage::AuthenticationCode => text.literal("인증 코드"),
        SecurityMessage::Cancel => text.literal("취소"),
        SecurityMessage::Enable => text.literal("사용"),
        SecurityMessage::TwoFactorOn => text.literal("2단계 인증이 켜졌습니다."),
        SecurityMessage::SaveCodes => text.literal("지금 이 코드를 저장하세요"),
        SecurityMessage::CodesText => text.literal("패스키나 인증 앱을 잃어버렸을 때 각 코드로 한 번씩 로그인할 수 있습니다. 다시 표시되지 않으니 비밀번호 관리자 같은 안전한 곳에 보관하세요."),
        SecurityMessage::SavedCodes => text.literal("코드를 저장했습니다"),
        SecurityMessage::AppOrRecoveryCode => text.literal("인증 코드 또는 복구 코드"),
        SecurityMessage::RecoveryCode => text.literal("복구 코드"),
        SecurityMessage::PasskeyInstead => text.literal("또는 코드 대신 패스키로 확인하세요."),
        SecurityMessage::AuthenticatorRemoved => text.literal("인증 앱을 제거했습니다."),
        SecurityMessage::AuthenticatorAdded => text.literal("인증 앱을 추가했습니다."),
        SecurityMessage::PasskeyAdded => text.literal("패스키를 추가했습니다."),
        SecurityMessage::PasskeyRemoved => text.literal("패스키를 제거했습니다."),
        SecurityMessage::SessionRevoked => text.literal("세션에서 로그아웃했습니다."),
        SecurityMessage::OtherSessionsRevoked => text.literal("다른 모든 세션에서 로그아웃했습니다."),
        SecurityMessage::SetupCancelled => text.literal("2단계 인증 설정을 취소했습니다."),
        SecurityMessage::IdentityLinked => text.literal("계정을 연결했습니다. 이제 이 계정으로 로그인할 수 있습니다."),
        SecurityMessage::IdentityUnlinked => text.literal("계정 연결을 해제했습니다."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. 앱에 ");
            text.argument(site);
            text.literal(" 추가");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.argument(provider);
            text.literal(" 연결 해제");
        }
        SecurityText::SignOutLabel(device) => {
            text.argument(device);
            text.literal(" 로그아웃");
        }
        SecurityText::WithPasskey(action) => {
            text.literal("패스키로 ");
            text.argument(action);
        }
    }
}
