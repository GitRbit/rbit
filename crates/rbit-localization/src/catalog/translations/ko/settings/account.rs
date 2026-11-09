//! Korean (`ko`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { Ko:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("비밀번호 변경"),
        AccountMessage::Title => text.literal("계정"),
        AccountMessage::Intro => text.literal("로그인 방법입니다. 변경하려면 현재 비밀번호가 필요합니다."),
        AccountMessage::Username => text.literal("사용자 이름"),
        AccountMessage::UsernameUpdated => text.literal("사용자 이름을 변경했습니다."),
        AccountMessage::UsernameHint => text.literal("이전 이름을 사용하는 프로필 링크와 Git 원격 저장소는 다른 사람이 그 이름을 사용하기 전까지 계속 작동합니다. 링크와 원격 저장소를 업데이트하세요. 새 사용자 이름이나 이메일로 로그인하세요."),
        AccountMessage::CurrentPassword => text.literal("현재 비밀번호"),
        AccountMessage::ChangeUsername => text.literal("사용자 이름 변경"),
        AccountMessage::VerificationSent => text.literal("인증 이메일을 보냈습니다. 받은 편지함을 확인하세요."),
        AccountMessage::Verified => text.literal("인증됨"),
        AccountMessage::Unverified => text.literal("인증되지 않음"),
        AccountMessage::VerifiedText => text.literal("이 주소가 본인의 주소임을 확인했습니다."),
        AccountMessage::PendingText => text.literal("이메일로 보내 드린 링크로 주소를 확인하세요."),
        AccountMessage::UnavailableText => text.literal("이 서버에서는 이메일을 사용할 수 없어 주소를 인증할 수 없습니다."),
        AccountMessage::Resend => text.literal("인증 이메일 다시 보내기"),
        AccountMessage::EmailAddress => text.literal("이메일 주소"),
        AccountMessage::EmailUpdated => text.literal("이메일 주소를 변경했습니다."),
        AccountMessage::Email => text.literal("이메일"),
        AccountMessage::EmailHintLead => text.literal("이 주소나 사용자 이름으로 로그인할 수 있습니다. 현재 주소:"),
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("이메일 변경"),
        AccountMessage::Password => text.literal("비밀번호"),
        AccountMessage::PasswordChanged => text.literal("비밀번호를 변경했습니다."),
        AccountMessage::NewPassword => text.literal("새 비밀번호"),
        AccountMessage::ConfirmPassword => text.literal("새 비밀번호 확인"),
        AccountMessage::ChangePassword => text.literal("비밀번호 변경"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal("관리자가 초기 비밀번호로 계정을 만들었습니다. ");
            text.argument(site);
            text.literal("에서 계속하기 전에 새 비밀번호를 정하세요.");
        }
        AccountText::PasswordHint(length) => {
            text.number(length);
            text.literal("자 이상이어야 합니다. 서로 관련 없는 단어 몇 개를 조합하면 기억하기 쉬우면서도 강력한 비밀번호가 됩니다.");
        }
    }
}
