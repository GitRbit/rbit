//! Spanish (`es`) entries for `catalog/settings/profile.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::profile::{
    AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage,
};

entries! { Es:
    ProfileMessage => MessageWriter |text| {
        ProfileMessage::Title => text.literal("Perfil"),
        ProfileMessage::IntroLead => text.literal("Se muestra en"),
        ProfileMessage::IntroLink => text.literal("tu página de perfil"),
        ProfileMessage::IntroTail => {
            text.literal("y junto a tu actividad. Todo lo que hay aquí es público y opcional.");
        }
        ProfileMessage::Updated => text.literal("Perfil actualizado."),
        ProfileMessage::DisplayName => text.literal("Nombre visible"),
        ProfileMessage::DisplayNameHint => {
            text.literal("Se muestra en lugar de tu nombre de usuario donde hay espacio.");
        }
        ProfileMessage::Bio => text.literal("Biografía"),
        ProfileMessage::BioHint => text.literal("Hasta 300 caracteres."),
        ProfileMessage::Website => text.literal("Sitio web"),
        ProfileMessage::WebsitePlaceholder => text.literal("https://"),
        ProfileMessage::Location => text.literal("Ubicación"),
        ProfileMessage::Save => text.literal("Guardar perfil"),
    }
    AppearanceMessage => MessageWriter |text| {
        AppearanceMessage::Title => text.literal("Apariencia"),
        AppearanceMessage::Intro => text.literal(
            "Tu tema se guarda en tu cuenta y te acompaña a cada dispositivo en el que inicies sesión. Pasa el \
             puntero por un tema para previsualizarlo; haz clic para quedártelo.",
        ),
    }
    NotificationsMessage => MessageWriter |text| {
        NotificationsMessage::Title => text.literal("Notificaciones"),
        NotificationsMessage::IntroLead => text.literal("Todo sigue apareciendo en"),
        NotificationsMessage::IntroLink => text.literal("tu bandeja de entrada"),
        NotificationsMessage::IntroTail => {
            text.literal(". Elige qué se envía también a tu dirección de correo electrónico.");
        }
        NotificationsMessage::Saved => text.literal("Preferencias guardadas."),
        NotificationsMessage::Unavailable => text.literal(
            "El correo electrónico no está disponible en este servidor, así que las notificaciones solo aparecen en \
             tu bandeja de entrada.",
        ),
        NotificationsMessage::VerifyLead => text.literal("Verifica"),
        NotificationsMessage::VerifyMiddle => text.literal("en la"),
        NotificationsMessage::VerifyLink => text.literal("configuración de la cuenta"),
        NotificationsMessage::VerifyTail => text.literal("para recibir correos de notificación."),
        NotificationsMessage::Email => text.literal("Correo electrónico"),
        NotificationsMessage::SendTo => text.literal("Enviar a"),
        NotificationsMessage::When => text.literal("Cuándo"),
        NotificationsMessage::DigestTime => text.literal("Hora del resumen"),
        NotificationsMessage::DigestHint => text.literal(
            "Los resúmenes omiten las conversaciones que ya leíste aquí. Cada correo tiene un enlace que los \
             desactiva con un clic.",
        ),
        NotificationsMessage::Save => text.literal("Guardar preferencias"),
        NotificationsMessage::Off => text.literal("Desactivado"),
        NotificationsMessage::OffText => text.literal("Las notificaciones solo aparecen en tu bandeja de entrada."),
        NotificationsMessage::Participating => text.literal("Participación"),
        NotificationsMessage::ParticipatingText => text.literal(
            "Las conversaciones que abriste, comentaste o revisaste, y cuando te mencionan, te asignan o te piden \
             una revisión.",
        ),
        NotificationsMessage::Everything => text.literal("Todo"),
        NotificationsMessage::EverythingText => {
            text.literal("También las conversaciones a las que te suscribiste sin participar.");
        }
        NotificationsMessage::Immediately => text.literal("Inmediatamente"),
        NotificationsMessage::ImmediatelyText => {
            text.literal("Un correo por cada notificación, en el momento en que ocurre.");
        }
        NotificationsMessage::Daily => text.literal("Resumen diario"),
        NotificationsMessage::DailyText => text.literal("Un correo al día con todo lo ocurrido desde el anterior."),
        NotificationsMessage::Weekly => text.literal("Resumen semanal"),
        NotificationsMessage::WeeklyText => {
            text.literal("Un correo cada lunes con todo lo de la semana anterior.");
        }
    }
    NotificationsText<'a> => MessageWriter |text| {
        NotificationsText::DefaultHour(hour) => {
            text.literal("Predeterminada (");
            text.argument(hour);
            text.literal(")");
        }
    }
    GalleryMessage => MessageWriter |text| {
        GalleryMessage::Show => text.literal("Mostrar"),
        GalleryMessage::Search => text.literal("Buscar temas"),
        GalleryMessage::BuiltIn => text.literal("Integrados"),
        GalleryMessage::Classic => text.literal("rbit Classic"),
        GalleryMessage::Empty => text.literal("Ningún tema coincide con tu búsqueda."),
        GalleryMessage::CreditLead => {
            text.literal("Los temas Tinted se generan a partir de los esquemas base16 y base24 de");
        }
        GalleryMessage::CreditLink => text.literal("tinted-theming"),
        GalleryMessage::CreditTail => text.literal("(MIT); pasa el puntero por un tema para ver su autor."),
        GalleryMessage::Licence => text.literal("Licencia y créditos"),
        GalleryMessage::LicenceTail => text.literal("."),
    }
    GalleryText => MessageWriter |text| {
        GalleryText::SearchCount(total) => {
            text.literal("Buscar entre ");
            text.number(total);
            text.literal(" temas");
        }
    }
}
