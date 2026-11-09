//! German (`de`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("Offen"),
        ThreadMessage::Closed => text.literal("Geschlossen"),
        ThreadMessage::Merged => text.literal("Zusammengeführt"),
        ThreadMessage::Draft => text.literal("Entwurf"),
        ThreadMessage::Title => text.literal("Titel"),
        ThreadMessage::Description => text.literal("Beschreibung"),
        ThreadMessage::Edit => text.literal("Bearbeiten"),
        ThreadMessage::Save => text.literal("Speichern"),
        ThreadMessage::Cancel => text.literal("Abbrechen"),
        ThreadMessage::Delete => text.literal("Löschen"),
        ThreadMessage::Optional => text.literal("optional"),
        ThreadMessage::MarkdownSupported => text.literal("Markdown wird unterstützt"),
        ThreadMessage::SignIn => text.literal("Anmelden"),
        ThreadMessage::LoadMore => text.literal("Mehr laden"),
        ThreadMessage::AddComment => text.literal("Kommentar hinzufügen"),
        ThreadMessage::CommentPlaceholder => text.literal(
            "Kommentar schreiben. Verwenden Sie @ für Personen und # für Issues oder Pull-Requests.",
        ),
        ThreadMessage::MentionSuggestions => text.literal("Vorschläge für Erwähnungen"),
        ThreadMessage::CommentHint => text.literal(
            "Markdown wird unterstützt. Mit @name, #123 oder owner/repo#123 verlinken Sie eine Person, ein Issue oder einen Pull-Request.",
        ),
        ThreadMessage::LockedHint => {
            text.literal("Diese Unterhaltung ist gesperrt; nur Mitarbeitende können kommentieren.");
        }
        ThreadMessage::CloseIssue => text.literal("Issue schließen"),
        ThreadMessage::ClosePull => text.literal("Pull-Request schließen"),
        ThreadMessage::CloseNotPlanned => text.literal("Als nicht geplant schließen"),
        ThreadMessage::ReopenIssue => text.literal("Issue erneut öffnen"),
        ThreadMessage::ReopenPull => text.literal("Pull-Request erneut öffnen"),
        ThreadMessage::Comment => text.literal("Kommentieren"),
        ThreadMessage::LockConversation => text.literal("Unterhaltung sperren"),
        ThreadMessage::UnlockConversation => text.literal("Unterhaltung entsperren"),
        ThreadMessage::Bot => text.literal("Bot"),
        ThreadMessage::Edited => text.literal("bearbeitet"),
        ThreadMessage::Author => text.literal("Autor"),
        ThreadMessage::Reply => text.literal("Antworten"),
        ThreadMessage::DeleteCommentConfirm => text.literal("Diesen Kommentar löschen?"),
        ThreadMessage::DeleteComment => text.literal("Kommentar löschen"),
        ThreadMessage::NoDescription => text.literal("Keine Beschreibung angegeben."),
    }
}
