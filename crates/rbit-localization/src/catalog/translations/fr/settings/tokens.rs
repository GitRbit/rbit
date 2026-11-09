//! French (`fr`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::TokensSentence;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};
use crate::plural::Plural;

entries! { Fr:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("Jetons d’accès"),
        TokensMessage::CopyNow => text.literal("Copiez votre nouveau jeton maintenant. Vous ne pourrez plus le voir."),
        TokensMessage::CopyToken => text.literal("Copier le jeton"),
        TokensMessage::Copy => text.literal("Copier"),
        TokensMessage::NewToken => text.literal("Nouveau jeton"),
        TokensMessage::Name => text.literal("Nom"),
        TokensMessage::NamePlaceholder => text.literal("p. ex. Ordinateur portable professionnel"),
        TokensMessage::NameHint => text.literal("Un nom pour le reconnaître plus tard."),
        TokensMessage::Scope => text.literal("Portée"),
        TokensMessage::ScopeHint => text.literal("Un jeton ne peut jamais faire plus que votre compte."),
        TokensMessage::Expiration => text.literal("Expiration"),
        TokensMessage::Generate => text.literal("Générer le jeton"),
        TokensMessage::YourTokens => text.literal("Vos jetons"),
        TokensMessage::EmptyTitle => text.literal("Aucun jeton d’accès"),
        TokensMessage::EmptyText => text.literal(
            "Générez un jeton pour cloner, récupérer et pousser en HTTPS sans votre mot de passe.",
        ),
        TokensMessage::Revoke => text.literal("Révoquer"),
        TokensMessage::NoExpiration => text.literal("Aucune expiration"),
        TokensMessage::OneDay => text.literal("1\u{a0}jour"),
        TokensMessage::NeverUsed => text.literal("Jamais utilisé"),
        TokensMessage::ReadDescription => text.literal("Cloner et récupérer\u{202f}; lire via l’API."),
        TokensMessage::WriteDescription => text.literal(
            "Également pousser, créer des dépôts et ajouter des étoiles via l’API.",
        ),
        TokensMessage::AdminDescription => text.literal(
            "Également modifier les paramètres et les webhooks des dépôts, et supprimer des dépôts via l’API.",
        ),
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("Jeton «\u{a0}");
            text.argument(name);
            text.literal("\u{a0}» créé");
        }
        TokenText::RevokeLabel(name) => {
            text.literal("Révoquer ");
            text.argument(name);
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(match text.plural(days) {
                Plural::One => " jour",
                _ => " jours",
            });
        }
        TokenText::CreatedOn(date) => {
            text.literal("Créé ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("Dernière utilisation ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("Expire ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("Expiré ");
            text.argument(date);
        }
    }
    TokensSentence => RichWriter |text| {
        TokensSentence::Intro => {
            text.literal("Utilisez un jeton à la place de votre mot de passe pour Git en HTTPS\u{a0}: saisissez votre nom d’utilisateur ");
            text.slot(0);
            text.literal(" et collez le jeton lorsque Git demande un mot de passe. L’API REST à l’adresse ");
            text.slot(1);
            text.literal(" l’accepte sous la forme ");
            text.slot(2);
            text.literal(". Les jetons commencent par ");
            text.slot(3);
            text.literal("\u{202f}; ceux des versions précédentes commencent par ");
            text.slot(4);
            text.literal(" et continuent de fonctionner.");
        }
    }
}
