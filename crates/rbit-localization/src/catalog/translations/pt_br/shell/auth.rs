//! Brazilian Portuguese (`pt-BR`) entries for `catalog/shell/auth.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::auth::{Attempt, AuthMessage};
use crate::plural::Plural;

entries! { PtBr:
    AuthMessage<'a> => MessageWriter |text| {
        AuthMessage::SignInTitle => text.literal("Entrar"),
        AuthMessage::SignUpTitle => text.literal("Criar conta"),
        AuthMessage::SignInTo(site) => {
            text.literal("Entrar em ");
            text.argument(site);
        }
        AuthMessage::SignInLede => text.literal("Use seu nome de usuário ou endereço de e-mail."),
        AuthMessage::UsernameOrEmail => text.literal("Nome de usuário ou e-mail"),
        AuthMessage::Password => text.literal("Senha"),
        AuthMessage::ForgotPassword => text.literal("Esqueceu a senha?"),
        AuthMessage::SignIn => text.literal("Entrar"),
        AuthMessage::Or => text.literal("ou"),
        AuthMessage::SignInWith(provider) => {
            text.literal("Entrar com ");
            text.argument(provider);
        }
        AuthMessage::NoAccount => text.literal("Não tem uma conta?"),
        AuthMessage::SignUp => text.literal("Criar conta"),
        AuthMessage::CreateYourAccount => text.literal("Crie sua conta"),
        AuthMessage::HostYourCode(site) => {
            text.literal("Hospede seu código em ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::Username => text.literal("Nome de usuário"),
        AuthMessage::UsernameHint => text.literal("Letras, dígitos, “-”, “_” e “.”"),
        AuthMessage::Email => text.literal("E-mail"),
        AuthMessage::MinimumLength(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.literal("Pelo menos ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " caractere.",
                _ => " caracteres.",
            });
        }
        AuthMessage::SetupToken => text.literal("Token de configuração"),
        AuthMessage::SetupTokenHint => {
            text.literal("Esta primeira conta administra o servidor. O token está no log do servidor.");
        }
        AuthMessage::CreateAccount => text.literal("Criar conta"),
        AuthMessage::RegistrationClosed => {
            text.literal("O cadastro está fechado neste servidor. Peça uma conta a um administrador.");
        }
        AuthMessage::HaveAccount => text.literal("Já tem uma conta?"),
        AuthMessage::TwoFactorTitle => text.literal("Autenticação em dois fatores"),
        AuthMessage::PasskeyOrApp => {
            text.literal("Use sua chave de acesso ou digite o código de 6 dígitos do seu aplicativo autenticador.");
        }
        AuthMessage::PasskeyOnly => {
            text.literal("Use sua chave de acesso ou chave de segurança para concluir a entrada.");
        }
        AuthMessage::AppOnly => text.literal("Digite o código de 6 dígitos do seu aplicativo autenticador."),
        AuthMessage::UsePasskey => text.literal("Usar uma chave de acesso"),
        AuthMessage::PasskeyUnsupported => {
            text.literal("Este navegador não pode usar chaves de acesso. Digite um código.");
        }
        AuthMessage::AuthenticationCode => text.literal("Código de autenticação"),
        AuthMessage::LostDevice => {
            text.literal("Perdeu seu dispositivo? Digite um dos seus códigos de recuperação.");
        }
        AuthMessage::RecoveryCode => text.literal("Código de recuperação"),
        AuthMessage::LostPasskey => {
            text.literal("Perdeu sua chave de acesso? Digite um dos seus códigos de recuperação.");
        }
        AuthMessage::VerifyCode => text.literal("Verificar código"),
        AuthMessage::NotYou => text.literal("Não é você?"),
        AuthMessage::DifferentAccount => text.literal("Entrar com outra conta"),
        AuthMessage::ProviderLede(provider, site) => {
            text.literal("Você entrou com ");
            text.argument(provider);
            text.literal(". Escolha um nome de usuário em ");
            text.argument(site);
            text.literal(".");
        }
        AuthMessage::VerifiedBy(provider) => {
            text.literal("Verificado por ");
            text.argument(provider);
            text.literal(". Outro endereço precisa ser confirmado por e-mail.");
        }
        AuthMessage::ConfirmByEmail => text.literal("Você receberá um e-mail para confirmar este endereço."),
        AuthMessage::ProviderPasswordNote(provider) => {
            text.literal("Você entrará com ");
            text.argument(provider);
            text.literal(
                "; sua conta não terá senha até você definir uma com um link de redefinição. Git por HTTPS exige \
                 um token de acesso pessoal.",
            );
        }
        AuthMessage::LinkInSettings(provider) => {
            text.literal("e vincule ");
            text.argument(provider);
            text.literal(" nas suas configurações de segurança.");
        }
        AuthMessage::SignInFailed => text.literal("Não foi possível entrar"),
        AuthMessage::BackToSignIn => text.literal("Voltar para a entrada"),
        AuthMessage::BackToSecuritySettings => text.literal("Voltar para as configurações de segurança"),
        AuthMessage::IncorrectCredentials => text.literal("Nome de usuário ou senha incorretos."),
        AuthMessage::Suspended => text.literal("Esta conta foi suspensa. Entre em contato com um administrador."),
        AuthMessage::SignInExpired => text.literal("Sua sessão expirou. Entre novamente."),
        AuthMessage::TooManyAttempts(attempt, seconds) => {
            text.literal(match attempt {
                Attempt::SignIn => "Muitas tentativas de entrada. Tente novamente em ",
                Attempt::SignUp => "Muitas tentativas de cadastro. Tente novamente em ",
            });
            if seconds < 60 {
                text.number(seconds);
                text.literal(match text.plural(seconds) {
                    Plural::One => " segundo.",
                    _ => " segundos.",
                });
            } else {
                let minutes = seconds.div_ceil(60);
                text.number(minutes);
                text.literal(match text.plural(minutes) {
                    Plural::One => " minuto.",
                    _ => " minutos.",
                });
            }
        }
    }
}
