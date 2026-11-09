//! Korean (`ko`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::ChromeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("본문으로 건너뛰기"),
        ChromeMessage::Main => text.literal("주 메뉴"),
        ChromeMessage::Explore => text.literal("탐색"),
        ChromeMessage::Language => text.literal("언어"),
        ChromeMessage::SwitchTheme => text.literal("라이트 모드와 다크 모드 전환"),
        ChromeMessage::ChooseTheme => text.literal("테마 선택"),
        ChromeMessage::Notifications => text.literal("알림"),
        ChromeMessage::Unread => text.literal("읽지 않은 알림이 있습니다"),
        ChromeMessage::NewRepository => text.literal("새 저장소"),
        ChromeMessage::AccountMenu => text.literal("계정 메뉴"),
        ChromeMessage::Issues => text.literal("이슈"),
        ChromeMessage::Pulls => text.literal("풀 리퀘스트"),
        ChromeMessage::Dashboard => text.literal("대시보드"),
        ChromeMessage::Profile => text.literal("프로필"),
        ChromeMessage::NewOrganization => text.literal("새 조직"),
        ChromeMessage::Settings => text.literal("설정"),
        ChromeMessage::Administration => text.literal("사이트 관리"),
        ChromeMessage::SignOut => text.literal("로그아웃"),
        ChromeMessage::SignIn => text.literal("로그인"),
        ChromeMessage::SignUp => text.literal("가입"),
        ChromeMessage::Menu => text.literal("메뉴"),
        ChromeMessage::CreateAccount => text.literal("계정 만들기"),
        ChromeMessage::Themes => text.literal("테마"),
        ChromeMessage::LightMode => text.literal("라이트 모드"),
        ChromeMessage::DarkMode => text.literal("다크 모드"),
        ChromeMessage::PoweredBy => text.literal("Powered by"),
        ChromeMessage::UpdateAvailable => text.literal("업데이트 사용 가능"),
        ChromeMessage::TimingHelp => text.literal("마지막 페이지 또는 업데이트의 서버 처리 시간"),
        ChromeMessage::RenderedIn => text.literal("렌더링 시간"),
        ChromeMessage::Footer => text.literal("바닥글"),
        ChromeMessage::Support => text.literal("지원"),
        ChromeMessage::Terms => text.literal("이용 약관"),
        ChromeMessage::Privacy => text.literal("개인정보 처리방침"),
        ChromeMessage::Status => text.literal("상태"),
        ChromeMessage::Home { site } => {
            text.argument(site);
            text.literal(" 홈");
        }
    }
}
