//! Korean (`ko`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { Ko:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("액세스 토큰"),
        TokensMessage::IntroLead => text.literal("HTTPS로 Git을 사용할 때 비밀번호 대신 토큰을 사용하세요. 사용자 이름에는"),
        TokensMessage::IntroUsername => text.literal("값을 입력하고, Git이 비밀번호를 물으면 토큰을 붙여 넣으세요."),
        TokensMessage::IntroApi => text.literal("경로의 REST API는 토큰을 다음 형식으로 받습니다:"),
        TokensMessage::IntroPrefix => text.literal(". 토큰은"),
        TokensMessage::IntroLegacy => text.literal("(으)로 시작합니다. 이전 릴리스에서 만든 토큰은"),
        TokensMessage::IntroTail => text.literal("접두사를 사용하며 계속 작동합니다."),
        TokensMessage::CopyNow => text.literal("지금 새 토큰을 복사하세요. 다시 볼 수 없습니다."),
        TokensMessage::CopyToken => text.literal("토큰 복사"),
        TokensMessage::Copy => text.literal("복사"),
        TokensMessage::NewToken => text.literal("새 토큰"),
        TokensMessage::Name => text.literal("이름"),
        TokensMessage::NamePlaceholder => text.literal("예: 업무용 노트북"),
        TokensMessage::NameHint => text.literal("나중에 알아볼 수 있는 이름입니다."),
        TokensMessage::Scope => text.literal("범위"),
        TokensMessage::ScopeHint => text.literal("토큰은 계정이 할 수 있는 것 이상을 할 수 없습니다."),
        TokensMessage::Expiration => text.literal("만료"),
        TokensMessage::Generate => text.literal("토큰 생성"),
        TokensMessage::YourTokens => text.literal("내 토큰"),
        TokensMessage::EmptyTitle => text.literal("액세스 토큰 없음"),
        TokensMessage::EmptyText => text.literal("비밀번호 없이 HTTPS로 클론, 페치, 푸시하려면 토큰을 생성하세요."),
        TokensMessage::Revoke => text.literal("폐기"),
        TokensMessage::NoExpiration => text.literal("만료 없음"),
        TokensMessage::OneDay => text.literal("1일"),
        TokensMessage::NeverUsed => text.literal("사용한 적 없음"),
        TokensMessage::ReadDescription => text.literal("클론과 페치, API를 통한 읽기."),
        TokensMessage::WriteDescription => text.literal("API를 통한 푸시, 저장소 만들기, 스타 표시도 가능합니다."),
        TokensMessage::AdminDescription => text.literal("API를 통한 저장소 설정 및 웹훅 변경, 저장소 삭제도 가능합니다."),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("토큰 “");
            text.argument(name);
            text.literal("”을(를) 생성했습니다");
        }
        TokenText::RevokeLabel(name) => {
            text.argument(name);
            text.literal(" 폐기");
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal("일");
        }
        TokenText::CreatedOn(date) => {
            text.literal("생성: ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("마지막 사용: ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("만료 예정: ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("만료됨: ");
            text.argument(date);
        }
    }
}
