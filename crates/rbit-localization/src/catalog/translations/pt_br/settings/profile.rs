//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/profile.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { PtBr:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("Perfil"),
        ProfileMessage::IntroLead => text.literal("Exibido na"),
        ProfileMessage::IntroLink => text.literal("sua página de perfil"),
        ProfileMessage::IntroTail => {
            text.literal("e ao lado da sua atividade. Tudo aqui é público e opcional.");
        }
        ProfileMessage::Updated => text.literal("Perfil atualizado."),
        ProfileMessage::DisplayName => text.literal("Nome de exibição"),
        ProfileMessage::DisplayNameHint => {
            text.literal("Exibido no lugar do seu nome de usuário quando houver espaço.");
        }
        ProfileMessage::Bio => text.literal("Biografia"),
        ProfileMessage::BioHint => text.literal("Até 300 caracteres."),
        ProfileMessage::Website => text.literal("Site"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("Localização"),
        ProfileMessage::Save => text.literal("Salvar perfil"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("Aparência"),
        AppearanceMessage::Intro => {
            text.literal(
                "Seu tema é salvo na sua conta e acompanha você em todos os dispositivos em que você entrar. Passe o \
                 mouse sobre um tema para pré-visualizá-lo; clique para mantê-lo.",
            );
        }
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("Notificações"),
        NotificationsMessage::IntroLead => text.literal("Tudo continua aparecendo na"),
        NotificationsMessage::IntroLink => text.literal("sua caixa de entrada"),
        NotificationsMessage::IntroTail => {
            text.literal(". Escolha o que também é enviado para o seu endereço de e-mail.");
        }
        NotificationsMessage::Saved => text.literal("Preferências salvas."),
        NotificationsMessage::Unavailable => {
            text.literal(
                "O e-mail não está disponível neste servidor, então as notificações aparecem apenas na sua caixa de \
                 entrada.",
            );
        }
        NotificationsMessage::VerifyLead => text.literal("Verifique "),
        NotificationsMessage::VerifyMiddle => text.literal(" nas "),
        NotificationsMessage::VerifyLink => text.literal("configurações da conta"),
        NotificationsMessage::VerifyTail => text.literal(" para receber e-mails de notificação."),
        NotificationsMessage::Email => text.literal("E-mail"),
        NotificationsMessage::SendTo => text.literal("Enviar para"),
        NotificationsMessage::When => text.literal("Quando"),
        NotificationsMessage::DigestTime => text.literal("Horário do resumo"),
        NotificationsMessage::DigestHint => {
            text.literal(
                "Os resumos deixam de fora as conversas que você já leu aqui. Todo e-mail tem um link que os desativa \
                 com um clique.",
            );
        }
        NotificationsMessage::Save => text.literal("Salvar preferências"),
        NotificationsMessage::Off => text.literal("Desativado"),
        NotificationsMessage::OffText => text.literal("As notificações aparecem apenas na sua caixa de entrada."),
        NotificationsMessage::Participating => text.literal("Participando"),
        NotificationsMessage::ParticipatingText => {
            text.literal(
                "Conversas que você abriu, comentou ou revisou, e quando você é mencionado, atribuído ou tem a revisão \
                 solicitada.",
            );
        }
        NotificationsMessage::Everything => text.literal("Tudo"),
        NotificationsMessage::EverythingText => {
            text.literal("Também as conversas em que você se inscreveu sem participar.");
        }
        NotificationsMessage::Immediately => text.literal("Imediatamente"),
        NotificationsMessage::ImmediatelyText => text.literal("Um e-mail para cada notificação, assim que ocorrer."),
        NotificationsMessage::Daily => text.literal("Resumo diário"),
        NotificationsMessage::DailyText => text.literal("Um e-mail por dia com tudo desde o anterior."),
        NotificationsMessage::Weekly => text.literal("Resumo semanal"),
        NotificationsMessage::WeeklyText => {
            text.literal("Um e-mail toda segunda-feira com tudo da semana anterior.");
        }
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("Padrão (");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("Mostrar"),
        GalleryMessage::Search => text.literal("Pesquisar temas"),
        GalleryMessage::BuiltIn => text.literal("Embutidos"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("Nenhum tema corresponde à sua pesquisa."),
        GalleryMessage::CreditLead => text.literal("Os temas tingidos são gerados a partir dos esquemas do"),
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::CreditTail => {
            text.literal("base16 e base24 (MIT); passe o mouse sobre um tema para ver o autor.");
        }
        GalleryMessage::Licence => text.literal("Licença e créditos"),
        GalleryMessage::LicenceTail => text.literal("."),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("Pesquisar ");
            text.number(total);
            text.literal(" temas");
        }
    }
}
