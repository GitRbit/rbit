//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { PtBr:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("Segurança"),
        SecurityMessage::Intro => {
            text.literal(
                "Proteja sua conta com uma chave de acesso ou um aplicativo autenticador e acompanhe onde você está \
                 conectado.",
            );
        }
        SecurityMessage::TwoFactor => text.literal("Autenticação em dois fatores"),
        SecurityMessage::On => text.literal("Ativada"),
        SecurityMessage::Off => text.literal("Desativada"),
        SecurityMessage::EnabledLead => {
            text.literal(
                "Ao entrar, é pedida uma chave de acesso ou um código depois da sua senha. Git por HTTPS exige um",
            );
        }
        SecurityMessage::TokenLink => text.literal("token de acesso pessoal"),
        SecurityMessage::EnabledTail => text.literal("; sua senha não funciona mais nele."),
        SecurityMessage::Disabled => {
            text.literal(
                "Adicione uma segunda etapa à entrada: uma chave de acesso, um aplicativo autenticador ou ambos.",
            );
        }
        SecurityMessage::Passkeys => text.literal("Chaves de acesso e chaves de segurança"),
        SecurityMessage::PasskeysText => {
            text.literal(
                "Confirme as entradas com sua impressão digital, rosto, PIN do dispositivo, celular ou uma chave física \
                 como uma YubiKey.",
            );
        }
        SecurityMessage::Remove => text.literal("Remover"),
        SecurityMessage::OnlyFactor => {
            text.literal(
                "Este é o seu único segundo fator: removê-lo desativa a autenticação em dois fatores e exclui seus \
                 códigos de recuperação.",
            );
        }
        SecurityMessage::PasskeyGone => text.literal("Você não poderá mais entrar com esta chave de acesso."),
        SecurityMessage::Name => text.literal("Nome"),
        SecurityMessage::PasskeyPlaceholder => text.literal("ex.: MacBook ou YubiKey"),
        SecurityMessage::AddPasskey => text.literal("Adicionar chave de acesso"),
        SecurityMessage::PasskeyUnsupported => {
            text.literal(
                "Adicionar uma chave de acesso exige um navegador compatível com chaves de acesso, com JavaScript \
                 ativado.",
            );
        }
        SecurityMessage::PasskeyUrlLead => text.literal("As chaves de acesso exigem que"),
        SecurityMessage::PasskeyUrlTail => text.literal("seja definido como o endereço deste site."),
        SecurityMessage::Authenticator => text.literal("Aplicativo autenticador"),
        SecurityMessage::AuthenticatorText => {
            text.literal("Use códigos de 6 dígitos de um aplicativo como 1Password, Aegis ou Google Authenticator.");
        }
        SecurityMessage::SetUp => text.literal("Configurar"),
        SecurityMessage::RemoveAuthenticator => text.literal("Remover aplicativo autenticador"),
        SecurityMessage::PasswordAlone => {
            text.literal(
                "Sua conta ficará protegida apenas pela sua senha, e seus códigos de recuperação serão excluídos.",
            );
        }
        SecurityMessage::PasskeysStay => {
            text.literal("Suas chaves de acesso continuam ativas como segundo fator.");
        }
        SecurityMessage::RecoveryCodes => text.literal("Códigos de recuperação"),
        SecurityMessage::Regenerate => text.literal("Gerar novos códigos de recuperação"),
        SecurityMessage::CodesStop => text.literal("Seus códigos de recuperação atuais deixarão de funcionar."),
        SecurityMessage::GenerateCodes => text.literal("Gerar novos códigos"),
        SecurityMessage::SignInMethods => text.literal("Métodos de entrada"),
        SecurityMessage::Password => text.literal("Senha"),
        SecurityMessage::NoPassword => text.literal("Sem senha"),
        SecurityMessage::WithPassword => text.literal("Entre com sua senha ou com uma conta vinculada."),
        SecurityMessage::LinkedOnlyLead => {
            text.literal(
                "Sua conta não tem senha: você entra com uma conta vinculada. Para definir uma, solicite um",
            );
        }
        SecurityMessage::ResetLink => text.literal("link de redefinição de senha"),
        SecurityMessage::LinkedOnlyMiddle => text.literal(". Git por HTTPS exige um"),
        SecurityMessage::LinkedOnlyTail => text.literal("."),
        SecurityMessage::NoLongerAvailable => text.literal("Não está mais disponível"),
        SecurityMessage::Unlink => text.literal("Desvincular"),
        SecurityMessage::OnlyWay => text.literal("Única forma de entrar"),
        SecurityMessage::NotLinked => text.literal("Não vinculada"),
        SecurityMessage::Link => text.literal("Vincular"),
        SecurityMessage::Sessions => text.literal("Sessões"),
        SecurityMessage::SignOutOthers => text.literal("Sair de todos os outros lugares"),
        SecurityMessage::ThisDevice => text.literal("Este dispositivo"),
        SecurityMessage::SignOut => text.literal("Sair"),
        SecurityMessage::RecentActivity => text.literal("Atividade recente"),
        SecurityMessage::NoActivity => text.literal("Ainda não há atividade"),
        SecurityMessage::NoActivityText => {
            text.literal("As entradas e alterações de segurança na sua conta aparecerão aqui.");
        }
        SecurityMessage::SetupTitle => text.literal("Configurar um aplicativo autenticador"),
        SecurityMessage::SetupIntro => {
            text.literal(
                "Escaneie o QR code com seu aplicativo autenticador e digite o código de 6 dígitos que ele mostrar.",
            );
        }
        SecurityMessage::CantScan => text.literal("Não consegue escanear? Digite esta chave"),
        SecurityMessage::CopyKey => text.literal("Copiar chave"),
        SecurityMessage::Copy => text.literal("Copiar"),
        SecurityMessage::TimeBased => text.literal("Baseado em tempo, 6 dígitos, a cada 30 segundos."),
        SecurityMessage::ConfirmCode => text.literal("2. Confirme um código"),
        SecurityMessage::AuthenticationCode => text.literal("Código de autenticação"),
        SecurityMessage::Cancel => text.literal("Cancelar"),
        SecurityMessage::Enable => text.literal("Ativar"),
        SecurityMessage::TwoFactorOn => text.literal("A autenticação em dois fatores está ativada."),
        SecurityMessage::SaveCodes => text.literal("Salve estes códigos agora"),
        SecurityMessage::CodesText => {
            text.literal(
                "Cada código permite entrar uma vez se você perder suas chaves de acesso ou seu aplicativo \
                 autenticador. Eles não serão exibidos novamente, então guarde-os em um lugar seguro, como um \
                 gerenciador de senhas.",
            );
        }
        SecurityMessage::SavedCodes => text.literal("Salvei meus códigos"),
        SecurityMessage::AppOrRecoveryCode => text.literal("Código de autenticação ou de recuperação"),
        SecurityMessage::RecoveryCode => text.literal("Código de recuperação"),
        SecurityMessage::PasskeyInstead => {
            text.literal("Ou confirme com uma das suas chaves de acesso em vez de um código.");
        }
        SecurityMessage::AuthenticatorRemoved => text.literal("Aplicativo autenticador removido."),
        SecurityMessage::AuthenticatorAdded => text.literal("Aplicativo autenticador adicionado."),
        SecurityMessage::PasskeyAdded => text.literal("Chave de acesso adicionada."),
        SecurityMessage::PasskeyRemoved => text.literal("Chave de acesso removida."),
        SecurityMessage::SessionRevoked => text.literal("Sessão encerrada."),
        SecurityMessage::OtherSessionsRevoked => text.literal("Todas as outras sessões foram encerradas."),
        SecurityMessage::SetupCancelled => {
            text.literal("Configuração da autenticação em dois fatores cancelada.");
        }
        SecurityMessage::IdentityLinked => text.literal("Conta vinculada. Agora você pode entrar com ela."),
        SecurityMessage::IdentityUnlinked => text.literal("Conta desvinculada."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. Adicione ");
            text.argument(site);
            text.literal(" ao seu aplicativo");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.literal("Desvincular ");
            text.argument(provider);
        }
        SecurityText::SignOutLabel(device) => {
            text.literal("Encerrar a sessão de ");
            text.argument(device);
        }
        SecurityText::WithPasskey(action) => {
            text.argument(action);
            text.literal(" com uma chave de acesso");
        }
    }
}
