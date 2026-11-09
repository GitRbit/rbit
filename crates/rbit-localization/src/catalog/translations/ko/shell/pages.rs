//! Korean (`ko`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};

entries! { Ko:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "페이지를 찾을 수 없습니다",
            Failure::Forbidden => "액세스가 거부되었습니다",
            Failure::BadRequest => "잘못된 요청",
            Failure::SlowDown => "요청이 너무 많습니다",
            Failure::Unavailable => "일시적으로 사용할 수 없음",
            Failure::Internal => "문제가 발생했습니다",
        }),
        PageMessage::NotFoundText => text.literal("찾으시는 페이지가 존재하지 않거나 액세스 권한이 없습니다."),
        PageMessage::UnavailableText => text.literal("서비스를 일시적으로 사용할 수 없습니다. 잠시 후 다시 시도하세요."),
        PageMessage::InternalText => text.literal("예기치 않은 오류가 발생했습니다. 오류는 로그에 기록되었습니다."),
        PageMessage::GoHome => text.literal("홈으로 이동"),
        PageMessage::Explore => text.literal("탐색"),
        PageMessage::Offline => text.literal("오프라인 상태입니다"),
        PageMessage::OfflineText => text.literal("서버에 연결할 수 없습니다. 연결 상태를 확인하고 다시 시도하세요."),
        PageMessage::Retry => text.literal("다시 시도"),
        PageMessage::Private => text.literal("비공개"),
        PageMessage::Archived => text.literal("보관됨"),
        PageMessage::Stars(count) => {
            text.literal("스타 ");
            text.number(count.unsigned_abs());
            text.literal("개");
        }
        PageMessage::Pagination => text.literal("페이지 매기기"),
        PageMessage::Previous => text.literal("이전"),
        PageMessage::Next => text.literal("다음"),
        PageMessage::PageOf(page, pages) => {
            text.number(u64::from(page));
            text.literal(" / ");
            text.number(u64::from(pages));
            text.literal(" 페이지");
        }
        PageMessage::EditProfile => text.literal("프로필 편집"),
        PageMessage::Joined(date) => {
            text.literal("가입일: ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("프로필"),
        PageMessage::Repositories => text.literal("저장소"),
        PageMessage::StarsTab => text.literal("스타"),
        PageMessage::NewRepository => text.literal("새 저장소"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" 님은 아직 스타를 표시한 저장소가 없습니다.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.literal("아직 ");
            text.argument(name);
            text.literal("의 공개 저장소가 없습니다.");
        }
    }
}
