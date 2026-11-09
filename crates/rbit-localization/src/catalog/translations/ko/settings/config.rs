//! Korean (`ko`) entries for `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

mod fields_a;
mod fields_b;
mod sections;

entries! { Ko:
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section, key } => return write(field(section, key).map(|(label, _)| label), text),
        ConfigText::Help { section, key } => return write(field(section, key).map(|(_, help)| help), text),
        ConfigText::Choice { section, key, value } => return write(sections::choice(section, key, value), text),
        ConfigText::SectionTitle(key) => return write(sections::section(key).map(|(title, _)| title), text),
        ConfigText::SectionHelp(key) => return write(sections::section(key).map(|(_, help)| help), text),
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
        ConfigMessage::RemoveProvider => text.literal("공급자 삭제"),
        ConfigMessage::AddProvider => text.literal("ID 공급자 추가"),
        ConfigMessage::NewProvider => text.literal("새 로그인 공급자"),
        ConfigMessage::OnlySection => text.literal("이 섹션만 업데이트됩니다."),
        ConfigMessage::Recovery => text.literal("복구 및 배포 기본값"),
        ConfigMessage::RecoveryLead => text.literal("저장된 설정 때문에 서버가 시작되지 않으면 서버 환경에 다음을 설정하세요:"),
        ConfigMessage::RecoveryTail => text.literal("그런 다음 서버를 다시 시작하고 여기서 재정의 설정을 수정하거나 삭제하세요."),
        ConfigMessage::RemovingOverrides => text.literal("재정의 설정을 삭제하면 다음에 다시 시작할 때 파일 및 환경 변수 구성이 복원됩니다. 실시간 브랜딩과 정책은 유지됩니다."),
        ConfigMessage::RemoveOverrides => text.literal("모든 배포 재정의 삭제"),
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

/// The label and help of a field, from either half of the field table.
fn field(section: &str, key: &str) -> Option<(&'static str, &'static str)> {
    fields_a::field(section, key).or_else(|| fields_b::field(section, key))
}

/// Writes `value` and returns `true`, or returns `false` for a key without an entry.
fn write(value: Option<&str>, text: &mut MessageWriter) -> bool {
    value.inspect(|value| text.literal(value)).is_some()
}
