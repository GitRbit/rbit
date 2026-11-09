//! Korean (`ko`) entries for `catalog/settings/keys.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Ko:
    KeysMessage => MessageWriter |text| {
        KeysMessage::Title => text.literal("SSH 키"),
        KeysMessage::IntroLead => text.literal("키를 사용하면 비밀번호 없이 SSH로 클론, 페치, 푸시할 수 있습니다. 공개 키 파일의 내용을 붙여 넣으세요. 공개 키 파일은 보통"),
        KeysMessage::IntroTail => text.literal("입니다."),
        KeysMessage::NewKey => text.literal("새 SSH 키"),
        KeysMessage::TitleLabel => text.literal("제목"),
        KeysMessage::Optional => text.literal("선택 사항"),
        KeysMessage::TitlePlaceholder => text.literal("예: 업무용 노트북"),
        KeysMessage::TitleHint => text.literal("기본값은 키의 주석입니다."),
        KeysMessage::PublicKey => text.literal("공개 키"),
        KeysMessage::KeyPlaceholder => text.literal("ssh-ed25519 AAAA… you@example.com"),
        KeysMessage::KeyHintLead => text.literal("Ed25519, ECDSA, RSA(2048비트 이상) 또는 보안 키("),
        KeysMessage::KeyHintMiddle => text.literal(")를 사용할 수 있습니다. 다음 명령으로 만드세요:"),
        KeysMessage::KeyHintTail => text.literal("."),
        KeysMessage::AddKey => text.literal("키 추가"),
        KeysMessage::YourKeys => text.literal("내 키"),
        KeysMessage::EmptyTitle => text.literal("SSH 키 없음"),
        KeysMessage::EmptyText => text.literal("SSH로 Git을 사용하려면 공개 키를 추가하세요."),
        KeysMessage::Delete => text.literal("삭제"),
        KeysMessage::TestLead => text.literal("다음 명령으로 키가 작동하는지 확인하세요:"),
        KeysMessage::TestTail => text.literal("."),
    }
    KeyLabel<'a> => MessageWriter |text| {
        KeyLabel::Delete(title) => {
            text.argument(title);
            text.literal(" 삭제");
        }
    }
}
