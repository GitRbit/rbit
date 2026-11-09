//! Korean (`ko`) entries for `catalog/settings/profile.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::GallerySentence;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { Ko:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("프로필"),
        ProfileMessage::IntroLead => text.literal("이 정보는"),
        ProfileMessage::IntroLink => text.literal("내 프로필 페이지"),
        ProfileMessage::IntroTail => text.literal("및 내 활동 옆에 표시됩니다. 여기의 모든 항목은 공개되며 선택 사항입니다."),
        ProfileMessage::Updated => text.literal("프로필을 업데이트했습니다."),
        ProfileMessage::DisplayName => text.literal("표시 이름"),
        ProfileMessage::DisplayNameHint => text.literal("공간이 있으면 사용자 이름 대신 표시됩니다."),
        ProfileMessage::Bio => text.literal("소개"),
        ProfileMessage::BioHint => text.literal("최대 300자."),
        ProfileMessage::Website => text.literal("웹사이트"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("위치"),
        ProfileMessage::Save => text.literal("프로필 저장"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("모양"),
        AppearanceMessage::Intro => text.literal("테마는 계정에 저장되며 로그인하는 모든 기기에 적용됩니다. 테마에 마우스를 올리면 미리 보고, 클릭하면 적용합니다."),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("알림"),
        NotificationsMessage::IntroLead => text.literal("모든 알림은 계속"),
        NotificationsMessage::IntroLink => text.literal("받은 알림함"),
        NotificationsMessage::IntroTail => text.literal("에 표시됩니다. 이메일 주소로도 받을 항목을 선택하세요."),
        NotificationsMessage::Saved => text.literal("설정을 저장했습니다."),
        NotificationsMessage::Unavailable => text.literal("이 서버에서는 이메일을 사용할 수 없어 알림은 받은 알림함에만 표시됩니다."),
        NotificationsMessage::VerifyLead => text.literal("알림 이메일을 받으려면 "),
        NotificationsMessage::VerifyMiddle => text.literal(" 주소를 "),
        NotificationsMessage::VerifyLink => text.literal("계정 설정"),
        NotificationsMessage::VerifyTail => text.literal(" 페이지에서 인증하세요."),
        NotificationsMessage::Email => text.literal("이메일"),
        NotificationsMessage::SendTo => text.literal("받는 주소"),
        NotificationsMessage::When => text.literal("보낼 시점"),
        NotificationsMessage::DigestTime => text.literal("요약 발송 시간"),
        NotificationsMessage::DigestHint => text.literal("요약 이메일에는 여기에서 이미 읽은 스레드가 포함되지 않습니다. 모든 이메일에는 한 번의 클릭으로 수신을 끌 수 있는 링크가 있습니다."),
        NotificationsMessage::Save => text.literal("설정 저장"),
        NotificationsMessage::Off => text.literal("끄기"),
        NotificationsMessage::OffText => text.literal("알림은 받은 알림함에만 표시됩니다."),
        NotificationsMessage::Participating => text.literal("참여 중인 항목"),
        NotificationsMessage::ParticipatingText => text.literal("내가 연 스레드, 댓글을 달거나 리뷰한 스레드, 그리고 멘션되거나 담당자로 지정되거나 리뷰를 요청받은 경우."),
        NotificationsMessage::Everything => text.literal("모든 항목"),
        NotificationsMessage::EverythingText => text.literal("참여하지 않고 구독만 한 스레드도 포함합니다."),
        NotificationsMessage::Immediately => text.literal("즉시"),
        NotificationsMessage::ImmediatelyText => text.literal("알림이 발생할 때마다 이메일을 한 통씩 보냅니다."),
        NotificationsMessage::Daily => text.literal("일일 요약"),
        NotificationsMessage::DailyText => text.literal("하루에 한 번, 지난 이메일 이후의 모든 내용을 보냅니다."),
        NotificationsMessage::Weekly => text.literal("주간 요약"),
        NotificationsMessage::WeeklyText => text.literal("매주 월요일, 지난주의 모든 내용을 보냅니다."),
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("기본값(");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("표시"),
        GalleryMessage::Search => text.literal("테마 검색"),
        GalleryMessage::BuiltIn => text.literal("기본 제공"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("검색과 일치하는 테마가 없습니다."),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::Licence => text.literal("라이선스 및 출처"),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("테마 ");
            text.number(total);
            text.literal("개 검색");
        }
    }
    GallerySentence => RichWriter |text| {
        GallerySentence::Credit => {
            text.literal("Tinted 테마는 ");
            text.slot(0);
            text.literal(" 프로젝트의 base16 및 base24 스킴(MIT)에서 생성되었습니다. 테마에 마우스를 올리면 작성자를 볼 수 있습니다. ");
            text.slot(1);
            text.literal(".");
        }
    }
}
