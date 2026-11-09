//! French (`fr`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};
use crate::plural::Plural;

entries! { Fr:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("Releases"),
        ReleaseLabel::NewRelease => text.literal("Nouvelle release"),
        ReleaseLabel::EditRelease => text.literal("Modifier la release"),
        ReleaseLabel::NoReleasesYet => text.literal("Aucune release pour l’instant"),
        ReleaseLabel::EmptyForWriters => text.literal(
            "Une release associe un tag à des notes et à des fichiers. Poussez un tag, puis créez une release pour celui-ci.",
        ),
        ReleaseLabel::EmptyForReaders => text.literal(
            "Les releases marquent les versions de ce projet avec des notes et des téléchargements.",
        ),
        ReleaseLabel::Draft => text.literal("Brouillon"),
        ReleaseLabel::PreRelease => text.literal("Préversion"),
        ReleaseLabel::Latest => text.literal("Dernière"),
        ReleaseLabel::Bot => text.literal("bot"),
        ReleaseLabel::Created => text.literal("créée"),
        ReleaseLabel::Edit => text.literal("Modifier"),
        ReleaseLabel::Downloads => text.literal("Téléchargements"),
        ReleaseLabel::SourceZip => text.literal("Code source (zip)"),
        ReleaseLabel::SourceTarGz => text.literal("Code source (tar.gz)"),
        ReleaseLabel::Files => text.literal("Fichiers"),
        ReleaseLabel::Remove => text.literal("Retirer"),
        ReleaseLabel::FileToAttach => text.literal("Fichier à joindre"),
        ReleaseLabel::Upload => text.literal("Téléverser"),
        ReleaseLabel::DeleteTitle => text.literal("Supprimer cette release"),
        ReleaseLabel::DeleteHintStart => text.literal("La release, ses notes et ses fichiers sont supprimés. Le tag"),
        ReleaseLabel::DeleteHintEnd => text.literal("est conservé."),
        ReleaseLabel::DeleteRelease => text.literal("Supprimer la release"),
        ReleaseLabel::FormLead => text.literal("Une release décrit un tag\u{a0}: ses notes et des fichiers à télécharger."),
        ReleaseLabel::Tag => text.literal("Tag"),
        ReleaseLabel::AnExisting => text.literal("Un"),
        ReleaseLabel::TagLink => text.literal("tag"),
        ReleaseLabel::OrNewTag => text.literal("existant ou un nouveau, créé à partir de la branche ci-dessous."),
        ReleaseLabel::CreateTagFrom => text.literal("Créer un nouveau tag à partir de"),
        ReleaseLabel::TargetHint => text.literal("utilisé uniquement si le tag n’existe pas"),
        ReleaseLabel::OneOfTags => text.literal("L’un des"),
        ReleaseLabel::TagsLink => text.literal("tags du dépôt"),
        ReleaseLabel::Title => text.literal("Titre"),
        ReleaseLabel::TitleHint => text.literal("facultatif, le tag par défaut"),
        ReleaseLabel::Notes => text.literal("Notes"),
        ReleaseLabel::IsPreRelease => text.literal("Il s’agit d’une préversion"),
        ReleaseLabel::PreReleaseHint => text.literal(
            "Marquée comme non prête pour la production\u{202f}; une release complète est préférée comme dernière release.",
        ),
        ReleaseLabel::SaveRelease => text.literal("Enregistrer la release"),
        ReleaseLabel::PublishRelease => text.literal("Publier la release"),
        ReleaseLabel::SaveDraft => text.literal("Enregistrer le brouillon"),
        ReleaseLabel::Cancel => text.literal("Annuler"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " téléchargement",
                _ => " téléchargements",
            });
        }
        ReleaseMessage::RemoveFile { name } => {
            text.literal("Retirer ");
            text.argument(name);
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("Jusqu’à ");
            text.argument(size);
            text.literal(" par fichier. Les téléchargements sont toujours servis comme fichiers, jamais affichés.");
        }
        ReleaseMessage::Uploaded { name } => {
            text.argument(name);
            text.literal(" a été téléversé.");
        }
        ReleaseMessage::Removed { name } => {
            text.argument(name);
            text.literal(" a été retiré.");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.literal("Modifier ");
            text.argument(tag);
        }
    }
}
