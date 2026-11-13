//! Korean (`ko`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;
use crate::catalog::shell::recovery::RecoverySentence;

entries! { Ko:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("비밀번호 재설정"),
        RecoveryMessage::ForgotLede => text.literal("계정의 이메일 주소를 입력하시면 새 비밀번호를 설정할 수 있는 링크를 보내 드립니다."),
        RecoveryMessage::ForgotWithoutEmail => text.literal("이 서버에서는 이메일을 사용할 수 없어 이메일로 비밀번호를 재설정할 수 없습니다. 관리자에게 새 비밀번호 설정을 요청하세요."),
        RecoveryMessage::CheckEmail => text.literal("이메일을 확인하세요"),
        RecoveryMessage::LinkSent => text.literal("해당 주소를 사용하는 계정이 있으면 새 비밀번호를 설정할 수 있는 링크를 보냈습니다. 링크는 한 번만 사용할 수 있으며 1시간 후에 만료됩니다."),
        RecoveryMessage::TryAgain => text.literal("다시 시도하세요"),
        RecoveryMessage::SendResetLink => text.literal("재설정 링크 보내기"),
        RecoveryMessage::RequestNewLink => text.literal("새 링크 요청"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal("이 서버에서는 이메일을 사용할 수 없어 새 링크를 보낼 수 없습니다. 관리자에게 새 비밀번호 설정을 요청하세요."),
        RecoveryMessage::ChooseNewPassword => text.literal("새 비밀번호 설정"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("대상 계정: ");
            text.argument(username);
            text.literal(". 앞으로 이 비밀번호로 로그인합니다.");
        }
        RecoveryMessage::NewPassword => text.literal("새 비밀번호"),
        RecoveryMessage::ConfirmNewPassword => text.literal("새 비밀번호 확인"),
        RecoveryMessage::ChangePassword => text.literal("비밀번호 변경"),
        RecoveryMessage::BackToSignIn => text.literal("로그인으로 돌아가기"),
        RecoveryMessage::PasswordChanged => text.literal("비밀번호가 변경되었습니다"),
        RecoveryMessage::PasswordChangedText => text.literal("새 비밀번호가 설정되었습니다. 다음 로그인부터 이 비밀번호를 사용하세요."),
        RecoveryMessage::LinkExpired => text.literal("만료된 링크입니다"),
        RecoveryMessage::ResetLinkExpiredText => text.literal("비밀번호 재설정 링크는 한 번만 사용할 수 있으며 1시간 후에 만료됩니다. 계속하려면 새 링크를 요청하세요."),
        RecoveryMessage::AccountSuspended => text.literal("정지된 계정입니다"),
        RecoveryMessage::AccountSuspendedText => text.literal("이 계정의 비밀번호는 재설정할 수 없습니다. 관리자에게 문의하세요."),
        RecoveryMessage::EmailVerified => text.literal("이메일 주소가 확인되었습니다"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.argument(address);
            text.literal(" 주소를 확인해 주셔서 감사합니다.");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal("확인 링크는 한 번만 사용할 수 있으며 2일 후에 만료되고, 주소가 바뀌면 더 이상 작동하지 않습니다. 로그인한 후 계정 설정에서 새 링크를 보내세요."),
        RecoveryMessage::GoToAccountSettings => text.literal("계정 설정으로 이동"),
        RecoveryMessage::SignIn => text.literal("로그인"),
        RecoveryMessage::ConfirmEmailTitle => text.literal("이메일 주소 확인"),
        RecoveryMessage::ConfirmEmailText(address) => {
            text.argument(address);
            text.literal("을(를) 계정의 이메일 주소로 확인합니다. 요청하지 않았다면 이 페이지를 닫으세요.");
        }
        RecoveryMessage::ConfirmEmail => text.literal("이메일 주소 확인"),
    }
    RecoverySentence => RichWriter |text| {
        RecoverySentence::NothingArrived => {
            text.literal("메일이 오지 않았나요? 스팸 폴더를 확인하거나 ");
            text.slot(0);
            text.literal(".");
        }
        RecoverySentence::RememberedSignIn => {
            text.literal("비밀번호가 기억나셨나요? ");
            text.slot(0);
        }
    }
}
