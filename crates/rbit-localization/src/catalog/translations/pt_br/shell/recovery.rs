//! Brazilian Portuguese (`pt-BR`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;

entries! { PtBr:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("Redefinir sua senha"),
        RecoveryMessage::ForgotLede => {
            text.literal(
                "Digite o endereço de e-mail da sua conta e enviaremos um link para você escolher uma nova senha.",
            );
        }
        RecoveryMessage::ForgotWithoutEmail => {
            text.literal(
                "O e-mail não está disponível neste servidor, então as senhas não podem ser redefinidas por e-mail. \
                 Peça a um administrador que defina uma nova senha para você.",
            );
        }
        RecoveryMessage::CheckEmail => text.literal("Verifique seu e-mail"),
        RecoveryMessage::LinkSent => {
            text.literal(
                "Se alguma conta usa esse endereço, enviamos para ele um link para escolher uma nova senha. O link \
                 funciona uma vez e expira em uma hora.",
            );
        }
        RecoveryMessage::NothingArrived => text.literal("Não chegou nada? Verifique sua pasta de spam ou "),
        RecoveryMessage::TryAgainEnd => text.literal("."),
        RecoveryMessage::TryAgain => text.literal("tente novamente"),
        RecoveryMessage::SendResetLink => text.literal("Enviar link de redefinição"),
        RecoveryMessage::RememberedIt => text.literal("Lembrou?"),
        RecoveryMessage::RequestNewLink => text.literal("Solicitar um novo link"),
        RecoveryMessage::NewLinkWithoutEmail => {
            text.literal(
                "O e-mail não está disponível neste servidor, então não é possível enviar um novo link. Peça a um \
                 administrador que defina uma nova senha para você.",
            );
        }
        RecoveryMessage::ChooseNewPassword => text.literal("Escolha uma nova senha"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("Para ");
            text.argument(username);
            text.literal(". Você vai usá-la para entrar a partir de agora.");
        }
        RecoveryMessage::NewPassword => text.literal("Nova senha"),
        RecoveryMessage::ConfirmNewPassword => text.literal("Confirmar nova senha"),
        RecoveryMessage::ChangePassword => text.literal("Alterar senha"),
        RecoveryMessage::BackToSignIn => text.literal("Voltar para a entrada"),
        RecoveryMessage::PasswordChanged => text.literal("Senha alterada"),
        RecoveryMessage::PasswordChangedText => {
            text.literal("Sua nova senha foi definida. Use-a na próxima vez que entrar.");
        }
        RecoveryMessage::LinkExpired => text.literal("Este link expirou"),
        RecoveryMessage::ResetLinkExpiredText => {
            text.literal(
                "Os links de redefinição de senha funcionam uma vez e expiram após uma hora. Solicite um novo para \
                 continuar.",
            );
        }
        RecoveryMessage::AccountSuspended => text.literal("Esta conta está suspensa"),
        RecoveryMessage::AccountSuspendedText => {
            text.literal("A senha dela não pode ser redefinida. Entre em contato com um administrador.");
        }
        RecoveryMessage::EmailVerified => text.literal("Endereço de e-mail verificado"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("Obrigado por confirmar ");
            text.argument(address);
            text.literal(".");
        }
        RecoveryMessage::VerifyLinkExpiredText => {
            text.literal(
                "Os links de verificação funcionam uma vez, expiram após dois dias e deixam de funcionar quando o \
                 endereço muda. Entre e envie um novo pelas configurações da sua conta.",
            );
        }
        RecoveryMessage::GoToAccountSettings => text.literal("Ir para as configurações da conta"),
        RecoveryMessage::SignIn => text.literal("Entrar"),
    }
}
