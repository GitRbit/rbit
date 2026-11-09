//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { PtBr:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("Atualize sua senha"),
        AccountMessage::Title => text.literal("Conta"),
        AccountMessage::Intro => text.literal("Como você entra. As alterações pedem sua senha atual."),
        AccountMessage::Username => text.literal("Nome de usuário"),
        AccountMessage::UsernameUpdated => text.literal("Nome de usuário atualizado."),
        AccountMessage::UsernameHint => {
            text.literal(
                "Links de perfil e remotos Git que usam seu nome antigo continuam funcionando até que alguém passe a \
                 usar esse nome. Atualize seus links e remotos. Entre com seu novo nome de usuário ou e-mail.",
            );
        }
        AccountMessage::CurrentPassword => text.literal("Senha atual"),
        AccountMessage::ChangeUsername => text.literal("Alterar nome de usuário"),
        AccountMessage::VerificationSent => {
            text.literal("E-mail de verificação enviado. Verifique sua caixa de entrada.");
        }
        AccountMessage::Verified => text.literal("Verificado"),
        AccountMessage::Unverified => text.literal("Não verificado"),
        AccountMessage::VerifiedText => text.literal("Você confirmou que este endereço é seu."),
        AccountMessage::PendingText => text.literal("Confirme o endereço com o link que enviamos por e-mail."),
        AccountMessage::UnavailableText => {
            text.literal("O e-mail não está disponível neste servidor, então o endereço não pode ser verificado.");
        }
        AccountMessage::Resend => text.literal("Reenviar e-mail de verificação"),
        AccountMessage::EmailAddress => text.literal("Endereço de e-mail"),
        AccountMessage::EmailUpdated => text.literal("Endereço de e-mail atualizado."),
        AccountMessage::Email => text.literal("E-mail"),
        AccountMessage::EmailHintLead => {
            text.literal("Você pode entrar com este endereço ou com seu nome de usuário. Atualmente:");
        }
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("Atualizar e-mail"),
        AccountMessage::Password => text.literal("Senha"),
        AccountMessage::PasswordChanged => text.literal("Senha alterada."),
        AccountMessage::NewPassword => text.literal("Nova senha"),
        AccountMessage::ConfirmPassword => text.literal("Confirmar nova senha"),
        AccountMessage::ChangePassword => text.literal("Alterar senha"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal(
                "Um administrador criou sua conta com uma senha inicial. Escolha uma nova senha antes de continuar \
                 para ",
            );
            text.argument(site);
            text.literal(".");
        }
        AccountText::PasswordHint(length) => {
            text.literal("Pelo menos ");
            text.number(length);
            text.literal(" caracteres. Algumas palavras sem relação entre si formam uma senha forte e fácil de lembrar.");
        }
    }
}
