//! Brazilian Portuguese (`pt-BR`) entries for `catalog/chrome.rs`.

use crate::catalog::chrome::ChromeMessage;
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    ChromeMessage<'a> => MessageWriter |text| {
        ChromeMessage::Skip => text.literal("Pular para o conteúdo"),
        ChromeMessage::Main => text.literal("Principal"),
        ChromeMessage::Explore => text.literal("Explorar"),
        ChromeMessage::SwitchTheme => text.literal("Alternar entre claro e escuro"),
        ChromeMessage::ChooseTheme => text.literal("Escolher um tema"),
        ChromeMessage::Notifications => text.literal("Notificações"),
        ChromeMessage::Unread => text.literal("Você tem notificações não lidas"),
        ChromeMessage::NewRepository => text.literal("Novo repositório"),
        ChromeMessage::AccountMenu => text.literal("Menu da conta"),
        ChromeMessage::Issues => text.literal("Issues"),
        ChromeMessage::Pulls => text.literal("Pull requests"),
        ChromeMessage::Dashboard => text.literal("Painel"),
        ChromeMessage::Profile => text.literal("Perfil"),
        ChromeMessage::NewOrganization => text.literal("Nova organização"),
        ChromeMessage::Settings => text.literal("Configurações"),
        ChromeMessage::Administration => text.literal("Administração do site"),
        ChromeMessage::SignOut => text.literal("Sair"),
        ChromeMessage::SignIn => text.literal("Entrar"),
        ChromeMessage::SignUp => text.literal("Criar conta"),
        ChromeMessage::Menu => text.literal("Menu"),
        ChromeMessage::CreateAccount => text.literal("Criar uma conta"),
        ChromeMessage::Themes => text.literal("Temas"),
        ChromeMessage::LightMode => text.literal("Modo claro"),
        ChromeMessage::DarkMode => text.literal("Modo escuro"),
        ChromeMessage::PoweredBy => text.literal("Desenvolvido com"),
        ChromeMessage::UpdateAvailable => text.literal("Atualização disponível"),
        ChromeMessage::TimingHelp => text.literal("Tempo do servidor para a última página ou atualização"),
        ChromeMessage::RenderedIn => text.literal("gerado em"),
        ChromeMessage::Footer => text.literal("Rodapé"),
        ChromeMessage::Support => text.literal("Suporte"),
        ChromeMessage::Terms => text.literal("Termos"),
        ChromeMessage::Privacy => text.literal("Privacidade"),
        ChromeMessage::Status => text.literal("Status"),
        ChromeMessage::Home { site } => {
            text.literal("Início de ");
            text.argument(site);
        }
    }
}
