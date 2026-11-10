//! Korean (`ko`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { Ko:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("이 인스턴스에 계정이 있는 모든 사용자입니다."),
        AdminPageMessage::UsersSearch => text.literal("사용자 이름, 이름 또는 이메일로 검색"),
        AdminPageMessage::NewUser => text.literal("새 사용자"),
        AdminPageMessage::NoUsers => text.literal("사용자를 찾을 수 없습니다"),
        AdminPageMessage::NoUsersText => text.literal("다른 검색어로 시도해 보세요."),
        AdminPageMessage::User => text.literal("사용자"),
        AdminPageMessage::Joined => text.literal("가입일"),
        AdminPageMessage::Admin => text.literal("관리자"),
        AdminPageMessage::Suspended => text.literal("정지됨"),
        AdminPageMessage::EmailVerified => text.literal("이메일 주소 인증됨"),
        AdminPageMessage::Verified => text.literal("인증됨"),
        AdminPageMessage::You => text.literal("나"),
        AdminPageMessage::RemoveAdmin => text.literal("관리자 권한 해제"),
        AdminPageMessage::MakeAdmin => text.literal("관리자로 지정"),
        AdminPageMessage::LiftSuspension => text.literal("정지 해제"),
        AdminPageMessage::Suspend => text.literal("정지"),
        AdminPageMessage::SendReset => text.literal("비밀번호 재설정 메일 보내기"),
        AdminPageMessage::SetPasswordMenu => text.literal("비밀번호 설정…"),
        AdminPageMessage::DeleteMenu => text.literal("삭제…"),
        AdminPageMessage::ResetSent => text.literal("비밀번호 재설정 이메일을 보냈습니다."),
        AdminPageMessage::PasswordSet => text.literal("비밀번호가 변경되었습니다."),
        AdminPageMessage::NewUserIntro => text.literal("가입이 닫혀 있어도 관리자는 계정을 만들 수 있습니다."),
        AdminPageMessage::Username => text.literal("사용자 이름"),
        AdminPageMessage::UsernameHint => text.literal("영문자, 숫자, '-', '_', '.'"),
        AdminPageMessage::Email => text.literal("이메일"),
        AdminPageMessage::InitialPassword => text.literal("초기 비밀번호"),
        AdminPageMessage::SiteAdministrator => text.literal("사이트 관리자"),
        AdminPageMessage::SiteAdministratorText => text.literal("모든 저장소와 이 관리 패널에 대한 전체 액세스 권한입니다."),
        AdminPageMessage::CreateUser => text.literal("사용자 만들기"),
        AdminPageMessage::RegistrationClosed => text.literal("현재 공개 가입이 닫혀 있으므로 이 방법으로만 사용자를 추가할 수 있습니다."),
        AdminPageMessage::SetPassword => text.literal("비밀번호 설정"),
        AdminPageMessage::ResetHint => text.literal("사용자가 직접 비밀번호를 정하게 하려면 사용자 목록에서 비밀번호 재설정 메일을 보내세요."),
        AdminPageMessage::DeleteUser => text.literal("사용자 삭제"),
        AdminPageMessage::DeleteUserLead => text.literal("영구 삭제 대상:"),
        AdminPageMessage::OrganizationsIntro => text.literal("공유 네임스페이스와 그 구성원입니다."),
        AdminPageMessage::SearchByName => text.literal("이름으로 검색"),
        AdminPageMessage::NoOrganizations => text.literal("조직을 찾을 수 없습니다"),
        AdminPageMessage::NoOrganizationsText => text.literal("조직이 생기면 여기에 표시됩니다."),
        AdminPageMessage::Organization => text.literal("조직"),
        AdminPageMessage::Members => text.literal("구성원"),
        AdminPageMessage::Created => text.literal("생성일"),
        AdminPageMessage::RepositoriesIntro => text.literal("공개 및 비공개를 포함한 모든 저장소이며, 최근 활동 순으로 표시됩니다."),
        AdminPageMessage::RepositoriesSearch => text.literal("소유자/이름 또는 설명으로 검색"),
        AdminPageMessage::NoRepositories => text.literal("저장소를 찾을 수 없습니다"),
        AdminPageMessage::NoRepositoriesText => text.literal("저장소가 생기면 여기에 표시됩니다."),
        AdminPageMessage::Repository => text.literal("저장소"),
        AdminPageMessage::Updated => text.literal("업데이트"),
        AdminPageMessage::DeleteRepository => text.literal("저장소 삭제"),
        AdminPageMessage::DeleteRepositoryText => text.literal("모든 Git 데이터를 포함해 저장소를 영구 삭제합니다. 이 작업은 되돌릴 수 없습니다."),
        AdminPageMessage::AuditIntro => text.literal("로그인, 2단계 인증 변경, 세션, 액세스 토큰, 관리자 작업이 최신순으로 표시됩니다."),
        AdminPageMessage::AuditSearch => text.literal("사용자, 대상 또는 IP 주소"),
        AdminPageMessage::Category => text.literal("분류"),
        AdminPageMessage::Filter => text.literal("필터"),
        AdminPageMessage::AllEvents => text.literal("모든 이벤트"),
        AdminPageMessage::Clear => text.literal("지우기"),
        AdminPageMessage::NoEvents => text.literal("이벤트 없음"),
        AdminPageMessage::NoEventsText => text.literal("아직 이 필터와 일치하는 항목이 없습니다."),
        AdminPageMessage::As => text.literal("시도한 계정:"),
        AdminPageMessage::By => text.literal("수행자:"),
        AdminPageMessage::On => text.literal("대상:"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("변경 사항은 인스턴스 전체에 즉시 적용됩니다."),
        InstanceSettingsMessage::Saved => text.literal("설정을 저장했습니다."),
        InstanceSettingsMessage::SiteName => text.literal("사이트 이름"),
        InstanceSettingsMessage::SiteNameHint => text.literal("헤더, 페이지 제목, 앱으로 설치했을 때 표시됩니다."),
        InstanceSettingsMessage::Branding => text.literal("브랜딩 및 정체성"),
        InstanceSettingsMessage::BrandingHint => text.literal("텍스트는 일반 텍스트입니다. 이미지와 링크에는 HTTPS URL 또는 리버스 프록시가 제공하는 로컬 /경로를 쓸 수 있습니다. 이미지 필드를 비워 두면 기본 제공 이미지를 사용합니다."),
        InstanceSettingsMessage::PrimaryColor => text.literal("기본 버튼 색상(비워 두면 테마 사용)"),
        InstanceSettingsMessage::PrimaryTextColor => text.literal("기본 버튼 텍스트 색상"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => text.literal("버튼 레이블을 읽기 쉽도록 대비가 충분한 색상을 선택하세요. 두 색상을 모두 설정하거나 모두 비워 두세요."),
        InstanceSettingsMessage::Description => text.literal("사이트 설명"),
        InstanceSettingsMessage::WelcomeTitle => text.literal("환영 제목(비워 두면 사이트 이름 사용)"),
        InstanceSettingsMessage::WelcomeMessage => text.literal("환영 메시지"),
        InstanceSettingsMessage::LogoUrl => text.literal("로고 URL"),
        InstanceSettingsMessage::FaviconUrl => text.literal("파비콘 URL"),
        InstanceSettingsMessage::AppIconUrl => text.literal("앱 아이콘 URL(정사각형, 512픽셀 이상)"),
        InstanceSettingsMessage::FooterText => text.literal("바닥글 텍스트"),
        InstanceSettingsMessage::SupportUrl => text.literal("지원 URL"),
        InstanceSettingsMessage::TermsUrl => text.literal("이용 약관 URL"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("개인정보 처리방침 URL"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("Powered by rbit 표시"),
        InstanceSettingsMessage::ShowVersion => text.literal("소프트웨어 버전 표시"),
        InstanceSettingsMessage::ShowFeatures => text.literal("환영 페이지에 자체 호스팅 기능 표시"),
        InstanceSettingsMessage::Explore => text.literal("탐색 페이지"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("켜짐"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("사이트를 둘러볼 수 있는 모든 사용자가 모든 공개 저장소를 검색하고 목록으로 볼 수 있습니다."),
        InstanceSettingsMessage::ExploreDisabled => text.literal("꺼짐"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("탐색 페이지를 찾을 수 없게 되고 관리자에게도 링크가 숨겨집니다. 공개 저장소는 계속 공개 상태로 유지됩니다."),
        InstanceSettingsMessage::Registration => text.literal("가입"),
        InstanceSettingsMessage::Open => text.literal("열림"),
        InstanceSettingsMessage::OpenText => text.literal("누구나 가입 페이지에서 계정을 만들 수 있습니다."),
        InstanceSettingsMessage::Closed => text.literal("닫힘"),
        InstanceSettingsMessage::ClosedText => text.literal("관리자만 계정을 만들 수 있습니다."),
        InstanceSettingsMessage::OrgCreation => text.literal("조직을 만들 수 있는 사용자"),
        InstanceSettingsMessage::AnyUser => text.literal("모든 사용자"),
        InstanceSettingsMessage::AnyUserText => text.literal("로그인한 모든 사용자가 조직을 만들 수 있습니다."),
        InstanceSettingsMessage::AdminsOnly => text.literal("관리자만"),
        InstanceSettingsMessage::AdminsOnlyText => text.literal("사용자를 기존 조직에 추가하는 것은 여전히 가능합니다."),
        InstanceSettingsMessage::DefaultVisibility => text.literal("기본 저장소 공개 범위"),
        InstanceSettingsMessage::Public => text.literal("공개"),
        InstanceSettingsMessage::PublicText => text.literal("변경하지 않으면 새 저장소는 모두에게 표시됩니다."),
        InstanceSettingsMessage::Private => text.literal("비공개"),
        InstanceSettingsMessage::PrivateText => text.literal("새 저장소는 비공개로 시작하며, 소유자가 나중에 공개로 바꿀 수 있습니다."),
        InstanceSettingsMessage::VisibilityHint => text.literal("새 저장소 양식에서 미리 선택됩니다."),
        InstanceSettingsMessage::DigestTime => text.literal("이메일 요약 발송 시간"),
        InstanceSettingsMessage::DigestHint => text.literal("시간을 선택하지 않은 사용자에게 일간 및 주간 알림 요약을 보내는 시간입니다. 주간 요약은 월요일에 발송됩니다."),
        InstanceSettingsMessage::Save => text.literal("설정 저장"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.number(length);
            text.literal("자 이상. 사용자에게 안전하게 전달하세요. 사용자는 처음 로그인할 때 비밀번호를 바꿔야 합니다.");
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal("새 비밀번호는 현재 비밀번호를 즉시 대체하며, 기존 비밀번호 재설정 링크는 더 이상 작동하지 않습니다. ");
            text.argument(name);
            text.literal(" 님에게 안전하게 전달하세요.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", 이 사용자의 저장소(");
            text.argument(repositories);
            text.literal(", 모든 Git 데이터 포함), 액세스 토큰, 조직 멤버십. 이 작업은 되돌릴 수 없습니다.");
        }
    }
}
