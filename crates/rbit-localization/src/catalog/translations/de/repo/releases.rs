//! German (`de`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};
use crate::plural::Plural;

entries! { De:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("Releases"),
        ReleaseLabel::NewRelease => text.literal("Neues Release"),
        ReleaseLabel::EditRelease => text.literal("Release bearbeiten"),
        ReleaseLabel::NoReleasesYet => text.literal("Noch keine Releases"),
        ReleaseLabel::EmptyForWriters => text.literal(
            "Ein Release verbindet einen Tag mit Notizen und Dateien. Pushen Sie einen Tag und erstellen Sie dann \
             ein Release dafür.",
        ),
        ReleaseLabel::EmptyForReaders => text.literal(
            "Releases kennzeichnen Versionen dieses Projekts mit Notizen und Downloads.",
        ),
        ReleaseLabel::Draft => text.literal("Entwurf"),
        ReleaseLabel::PreRelease => text.literal("Vorabversion"),
        ReleaseLabel::Latest => text.literal("Neueste"),
        ReleaseLabel::Bot => text.literal("Bot"),
        ReleaseLabel::Created => text.literal("erstellt"),
        ReleaseLabel::Edit => text.literal("Bearbeiten"),
        ReleaseLabel::Downloads => text.literal("Downloads"),
        ReleaseLabel::SourceZip => text.literal("Quellcode (zip)"),
        ReleaseLabel::SourceTarGz => text.literal("Quellcode (tar.gz)"),
        ReleaseLabel::Files => text.literal("Dateien"),
        ReleaseLabel::Remove => text.literal("Entfernen"),
        ReleaseLabel::FileToAttach => text.literal("Anzuhängende Datei"),
        ReleaseLabel::Upload => text.literal("Hochladen"),
        ReleaseLabel::DeleteTitle => text.literal("Dieses Release löschen"),
        ReleaseLabel::DeleteHintStart => {
            text.literal("Das Release, seine Notizen und Dateien werden entfernt. Der Tag");
        }
        ReleaseLabel::DeleteHintEnd => text.literal("bleibt bestehen."),
        ReleaseLabel::DeleteRelease => text.literal("Release löschen"),
        ReleaseLabel::FormLead => text.literal(
            "Ein Release beschreibt einen Tag: mit Notizen und Dateien zum Herunterladen.",
        ),
        ReleaseLabel::Tag => text.literal("Tag"),
        ReleaseLabel::AnExisting => text.literal("Ein bestehender"),
        ReleaseLabel::TagLink => text.literal("Tag"),
        ReleaseLabel::OrNewTag => text.literal("oder ein neuer, der aus dem Branch unten erstellt wird."),
        ReleaseLabel::CreateTagFrom => text.literal("Neuen Tag erstellen aus"),
        ReleaseLabel::TargetHint => text.literal("nur verwendet, wenn der Tag nicht existiert"),
        ReleaseLabel::OneOfTags => text.literal("Einer der vorhandenen"),
        ReleaseLabel::TagsLink => text.literal("Tags"),
        ReleaseLabel::Title => text.literal("Titel"),
        ReleaseLabel::TitleHint => text.literal("optional, standardmäßig der Tag"),
        ReleaseLabel::Notes => text.literal("Notizen"),
        ReleaseLabel::IsPreRelease => text.literal("Dies ist eine Vorabversion"),
        ReleaseLabel::PreReleaseHint => text.literal(
            "Als nicht produktionsreif gekennzeichnet; als neuestes Release wird ein vollständiges Release \
             bevorzugt.",
        ),
        ReleaseLabel::SaveRelease => text.literal("Release speichern"),
        ReleaseLabel::PublishRelease => text.literal("Release veröffentlichen"),
        ReleaseLabel::SaveDraft => text.literal("Entwurf speichern"),
        ReleaseLabel::Cancel => text.literal("Abbrechen"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Download",
                _ => " Downloads",
            });
        }
        ReleaseMessage::RemoveFile { name } => {
            text.argument(name);
            text.literal(" entfernen");
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("Bis zu ");
            text.argument(size);
            text.literal(" pro Datei. Downloads werden immer als Dateien ausgeliefert, nie angezeigt.");
        }
        ReleaseMessage::Uploaded { name } => {
            text.argument(name);
            text.literal(" wurde hochgeladen.");
        }
        ReleaseMessage::Removed { name } => {
            text.argument(name);
            text.literal(" wurde entfernt.");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.argument(tag);
            text.literal(" bearbeiten");
        }
    }
}
