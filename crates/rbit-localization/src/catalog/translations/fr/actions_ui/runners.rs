//! French (`fr`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("Runners"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal(
                "Les runners de l’instance prennent en charge les jobs des propriétaires qu’un administrateur autorise, \
                 ou de tous les propriétaires avec ",
            );
            text.argument(setting);
            text.literal(".");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("Jeton d’enregistrement créé"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal(
                "Copiez-le maintenant\u{a0}: il ne sera plus affiché. Sur la machine qui doit exécuter les jobs, lancez \
                 la commande ci-dessous et collez le jeton lorsqu’elle le demande, puis démarrez le runner avec ",
            );
            text.argument(command);
            text.literal(".");
        }
        ActionsRunnersMessage::Token => text.literal("Jeton"),
        ActionsRunnersMessage::CopyToken => text.literal("Copier le jeton"),
        ActionsRunnersMessage::Copy => text.literal("Copier"),
        ActionsRunnersMessage::Command => text.literal("Commande"),
        ActionsRunnersMessage::CopyCommand => text.literal("Copier la commande"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("Les étiquettes déterminent comment les jobs s’exécutent\u{a0}: ");
            text.argument(container);
            text.literal(" s’exécute dans Docker ou Podman, ");
            text.argument(host);
            text.literal(
                " s’exécute directement sur la machine, sans aucune isolation — uniquement pour des dépôts de confiance.",
            );
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "ce dépôt"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "vos dépôts"),
        ActionsRunnersMessage::InstanceIntro { program } => {
            intro(text, program, None, "tous les propriétaires autorisés à utiliser les runners de l’instance");
        }
        ActionsRunnersMessage::NoRunners => text.literal("Aucun runner pour l’instant"),
        ActionsRunnersMessage::NoRunnersHint => text.literal(
            "Créez un jeton d’enregistrement ci-dessous et enregistrez un runner avec celui-ci. Les jobs attendent qu’un \
             runner portant leurs étiquettes soit en ligne.",
        ),
        ActionsRunnersMessage::Online => text.literal("en ligne"),
        ActionsRunnersMessage::Offline => text.literal("hors ligne"),
        ActionsRunnersMessage::Disabled => text.literal("désactivé"),
        ActionsRunnersMessage::NeverSeen => text.literal("jamais vu"),
        ActionsRunnersMessage::OwnerScope => text.literal("propriétaire"),
        ActionsRunnersMessage::InstanceScope => text.literal("instance"),
        ActionsRunnersMessage::Ephemeral => text.literal("éphémère"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("hôte — aucune isolation"),
        ActionsRunnersMessage::HostWarning => text.literal(
            "Les jobs portant ses étiquettes d’hôte s’exécutent sous l’utilisateur du runner, sans aucune isolation",
        ),
        ActionsRunnersMessage::Capacity { count } => {
            text.number(count);
            text.literal(" simultanément");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("vu ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("Activer"),
        ActionsRunnersMessage::Disable => text.literal("Désactiver"),
        ActionsRunnersMessage::Remove => text.literal("Retirer"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.literal("Retirer ");
            text.argument(name);
        }
        ActionsRunnersMessage::TokensTitle => text.literal("Jetons d’enregistrement"),
        ActionsRunnersMessage::TokensIntro => text.literal(
            "Un runner échange une seule fois un jeton d’enregistrement contre ses propres identifiants. Les jetons \
             sont à usage unique et expirent, sauf si vous en décidez autrement.",
        ),
        ActionsRunnersMessage::ExpiresAfter => text.literal("Expire après"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("Uniquement ces étiquettes"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(facultatif)"),
        ActionsRunnersMessage::Reusable => text.literal(
            "Permettre à un nombre illimité de runners de s’enregistrer avec ce jeton jusqu’à son expiration",
        ),
        ActionsRunnersMessage::NewToken => text.literal("Nouveau jeton d’enregistrement"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("Créé ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("expire ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("plus utilisable"),
        ActionsRunnersMessage::Revoked => text.literal("révoqué"),
        ActionsRunnersMessage::Expired => text.literal("expiré"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("utilisations illimitées"),
        ActionsRunnersMessage::UsedUp => text.literal("épuisé"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " utilisation restante",
                _ => " utilisations restantes",
            });
        }
        ActionsRunnersMessage::Revoke => text.literal("Révoquer"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " heure",
                _ => " heures",
            });
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " jour",
                _ => " jours",
            });
        }
    }
}

/// « Des machines exécutant {program} qui prennent en charge les jobs de {owner ou sujet}. »
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("Des machines exécutant ");
    text.argument(program);
    text.literal(" qui prennent en charge les jobs de ");
    match owner {
        Some(owner) => text.argument(owner),
        None => text.literal(subject),
    }
    text.literal(".");
}
