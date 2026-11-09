//! Korean (`ko`) entries for `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares `write`, the field labels and help of one part of the schema, from arms
/// written like the table below so the coverage report counts them.
macro_rules! config_entries {
    (ConfigText<$lifetime:lifetime> => MessageWriter |$text:ident| { $($arms:tt)* }) => {
        /// Writes a field's label or help, or returns `false` when this part lacks it.
        #[allow(clippy::too_many_lines, reason = "one arm per field label and help")]
        pub(super) fn write(message: ConfigText<'_>, $text: &mut MessageWriter) -> bool {
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_a;
mod fields_b;

entries! { Ko:
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::SectionTitle("updates") => text.literal("소프트웨어 업데이트 및 개인정보"),
        ConfigText::SectionTitle("server") => text.literal("서버"),
        ConfigText::SectionTitle("storage") => text.literal("스토리지"),
        ConfigText::SectionTitle("security") => text.literal("보안"),
        ConfigText::SectionTitle("repository") => text.literal("저장소"),
        ConfigText::SectionTitle("mail") => text.literal("이메일 전송"),
        ConfigText::SectionTitle("ssh") => text.literal("SSH"),
        ConfigText::SectionTitle("auth") => text.literal("로그인 공급자"),
        ConfigText::SectionTitle("branding") => text.literal("브랜딩 기본값"),
        ConfigText::SectionTitle("notifications") => text.literal("이메일 알림"),
        ConfigText::SectionTitle("operations") => text.literal("운영"),
        ConfigText::SectionTitle("api") => text.literal("REST API"),
        ConfigText::SectionTitle("webhooks") => text.literal("웹훅"),
        ConfigText::SectionTitle("releases") => text.literal("릴리스"),
        ConfigText::SectionTitle("mirrors") => text.literal("풀 미러"),
        ConfigText::SectionTitle("push_mirrors") => text.literal("푸시 미러"),
        ConfigText::SectionTitle("imports") => text.literal("가져오기"),
        ConfigText::SectionTitle("issues") => text.literal("이슈 및 받은 알림함"),
        ConfigText::SectionTitle("pulls") => text.literal("풀 리퀘스트"),
        ConfigText::SectionTitle("editor") => text.literal("브라우저 편집기"),
        ConfigText::SectionTitle("lfs") => text.literal("Git LFS"),
        ConfigText::SectionTitle("actions") => text.literal("Actions"),
        ConfigText::SectionHelp("updates") => text.literal("독립적인 백그라운드 릴리스 확인과 실행 중인 버전 공개 설정입니다. 변경 사항은 다시 시작한 후 적용됩니다."),
        ConfigText::SectionHelp("server") => text.literal("공개 주소, 수신 설정, 인스턴스 기본값입니다."),
        ConfigText::SectionHelp("storage") => text.literal("데이터 디렉터리를 확인합니다. 스토리지를 옮기려면 오프라인 상태에서 배포 구성을 사용하세요."),
        ConfigText::SectionHelp("security") => text.literal("세션, 비밀번호, 속도 제한, 신뢰할 수 있는 리버스 프록시입니다."),
        ConfigText::SectionHelp("repository") => text.literal("Git 저장소의 기본값과 한도입니다."),
        ConfigText::SectionHelp("mail") => text.literal("보내는 사람 정보, SMTP 연결, 전송 시간 제한을 구성합니다."),
        ConfigText::SectionHelp("ssh") => text.literal("내장 Git SSH 서버, 공개 클론 주소, 연결 한도입니다."),
        ConfigText::SectionHelp("auth") => text.literal("OIDC 및 GitHub ID 공급자를 연결합니다."),
        ConfigText::SectionHelp("branding") => text.literal("배포 기본값입니다. 인스턴스 전체에 즉시 적용하려면 브랜딩 및 정책을 사용하세요."),
        ConfigText::SectionHelp("notifications") => text.literal("알림 이메일을 켜고 기본 구독 정책을 선택합니다."),
        ConfigText::SectionHelp("operations") => text.literal("메트릭, 유지 관리, 백그라운드 작업, 캐시입니다."),
        ConfigText::SectionHelp("api") => text.literal("API 사용 여부와 페이지 한도입니다."),
        ConfigText::SectionHelp("webhooks") => text.literal("외부 전송, 재시도, 보관 설정입니다."),
        ConfigText::SectionHelp("releases") => text.literal("릴리스 에셋 크기, 개수, 업로드 시간 제한입니다."),
        ConfigText::SectionHelp("mirrors") => text.literal("페치 일정, 시간 제한, 네트워크 접근입니다."),
        ConfigText::SectionHelp("push_mirrors") => text.literal("푸시 일정, 시간 제한, 네트워크 접근입니다."),
        ConfigText::SectionHelp("imports") => text.literal("저장소 가져오기, 동시 실행 수, 작성자 연결입니다."),
        ConfigText::SectionHelp("issues") => text.literal("커밋 검사와 앱 내 알림 보관입니다."),
        ConfigText::SectionHelp("pulls") => text.literal("병합 한도, 백그라운드 검사, 작성자 주소입니다."),
        ConfigText::SectionHelp("editor") => text.literal("브라우저 업로드 크기, 개수, 시간 제한입니다."),
        ConfigText::SectionHelp("lfs") => text.literal("대용량 파일 스토리지, 객체 크기, 토큰 유효 기간입니다."),
        ConfigText::SectionHelp("actions") => text.literal("외부 러너에서 실행하는 CI/CD, 보관, 한도, 시간 제한입니다."),
        ConfigText::Choice { section: "mail", key: "transport", value: "disabled" } => text.literal("사용 안 함"),
        ConfigText::Choice { section: "mail", key: "transport", value: "log" } => text.literal("개발용 로그"),
        ConfigText::Choice { section: "mail", key: "transport", value: "smtp" } => text.literal("SMTP 서버"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "starttls" } => text.literal("STARTTLS(권장)"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "tls" } => text.literal("암시적 TLS"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "none" } => text.literal("암호화 없음"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "off" } => text.literal("끄기"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "participating" } => text.literal("참여한 스레드"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "all" } => text.literal("구독한 모든 스레드"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "none" } => text.literal("원래 작성자를 텍스트로 유지"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email" } => text.literal("인증된 이메일로 연결"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email_or_username" } => text.literal("인증된 이메일 또는 사용자 이름으로 연결"),
        ConfigText::Choice { section: "provider", key: "kind", value: "oidc" } => text.literal("OpenID Connect"),
        ConfigText::Choice { section: "provider", key: "kind", value: "github" } => text.literal("GitHub / GitHub Enterprise"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "true" } => text.literal("인스턴스 가입 정책 따르기"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "false" } => text.literal("기존 계정만"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "always" } => text.literal("가입이 닫혀 있어도 계정 생성 허용"),
        field => return fields_a::write(field, text) || fields_b::write(field, text),
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("구성"),
        ConfigMessage::Intro => text.literal("기능별로 정리된 설정으로 자체 호스팅 인스턴스를 관리합니다."),
        ConfigMessage::RestartServer => text.literal("서버 다시 시작"),
        ConfigMessage::Saved => text.literal("설정을 저장했습니다. 배포 변경 사항을 적용하려면 서버를 다시 시작하세요."),
        ConfigMessage::RestartRequired => text.literal("다시 시작 필요"),
        ConfigMessage::EnvironmentWins => text.literal("환경 변수가 우선합니다."),
        ConfigMessage::LivePolicies => text.literal("브랜딩 및 정책 — 즉시 적용"),
        ConfigMessage::SettingsSection => text.literal("설정 섹션"),
        ConfigMessage::OpenSection => text.literal("섹션 열기"),
        ConfigMessage::Sections => text.literal("구성 섹션"),
        ConfigMessage::LiveOverrides => text.literal("저장된 실시간 브랜딩 및 가입 정책이 이 배포 기본값보다 우선합니다."),
        ConfigMessage::IdentityProviders => text.literal("ID 공급자"),
        ConfigMessage::ProvidersHint => text.literal("먼저 공급자 카드를 추가하거나 삭제한 다음, 새 자격 증명을 입력하고 저장하세요. 공급자 변경 사항은 섹션을 저장할 때까지 이 양식에만 남아 있습니다."),
        ConfigMessage::CallbackLead => text.literal("각 공급자에 콜백 URL을 등록하세요. 공개 URL 뒤에 다음 경로를 붙인 주소입니다:"),
        ConfigMessage::CallbackTail => text.literal(" (PROVIDER_ID는 아래 ID로 바꾸세요). OpenID Connect는 발급자 URL을, GitHub Enterprise는 Enterprise URL을 설정하세요."),
        ConfigMessage::NoProviders => text.literal("구성된 외부 로그인 공급자가 없습니다. 로컬 로그인을 사용할 수 있습니다."),
        ConfigMessage::RemoveProvider => text.literal("공급자 제거"),
        ConfigMessage::AddProvider => text.literal("ID 공급자 추가"),
        ConfigMessage::NewProvider => text.literal("새 로그인 공급자"),
        ConfigMessage::OnlySection => text.literal("이 섹션만 업데이트됩니다."),
        ConfigMessage::Recovery => text.literal("복구 및 배포 기본값"),
        ConfigMessage::RecoveryLead => text.literal("저장된 설정 때문에 서버가 시작되지 않으면 서버 환경에 다음을 설정하세요:"),
        ConfigMessage::RecoveryTail => text.literal("그런 다음 서버를 다시 시작하고 여기서 재정의 설정을 수정하거나 삭제하세요."),
        ConfigMessage::RemovingOverrides => text.literal("재정의 설정을 삭제하면 다음에 다시 시작할 때 파일 및 환경 변수 구성이 복원됩니다. 실시간 브랜딩과 정책은 유지됩니다."),
        ConfigMessage::RemoveOverrides => text.literal("모든 배포 재정의 제거"),
        ConfigMessage::Optional => text.literal("(선택 사항)"),
        ConfigMessage::OnePerLine => text.literal("한 줄에 항목 하나씩 입력하세요."),
        ConfigMessage::ProviderDefaults => text.literal("공급자 기본값 사용"),
        ConfigMessage::Configured => text.literal("구성됨 — 유지하려면 비워 두세요"),
        ConfigMessage::NotConfigured => text.literal("구성되지 않음"),
        ConfigMessage::SecretHint => text.literal("현재 자격 증명을 유지하려면 비워 두세요. 새 자격 증명은 오류가 발생한 후 다시 표시되지 않으므로 양식을 수정할 때 다시 입력하세요. 인스턴스 데이터베이스와 백업을 안전하게 보호하세요."),
        ConfigMessage::ClearSecret => text.literal("이 자격 증명 지우기"),
        ConfigMessage::BlankDefault => text.literal("기본값을 사용하려면 비워 두세요."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.argument(section);
            text.literal(" 설정 저장");
        }
    }
}
