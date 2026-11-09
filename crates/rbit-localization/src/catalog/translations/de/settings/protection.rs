//! German (`de`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};
use crate::plural::Plural;

entries! { De:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("Setzen Sie zur Bestätigung das Häkchen."),
        ProtectionMessage::Settings => text.literal("Einstellungen"),
        ProtectionMessage::Title => text.literal("Branch-Schutz"),
        ProtectionMessage::IntroLead => text.literal("Regeln gelten für jeden Push nach"),
        ProtectionMessage::IntroTail => text.literal(
            ", über HTTP und SSH, auch für Administratoren, sofern eine Regel ihnen keine Umgehung erlaubt.",
        ),
        ProtectionMessage::AddRule => text.literal("Regel hinzufügen"),
        ProtectionMessage::Deleted => text.literal("Regel gelöscht."),
        ProtectionMessage::Empty => text.literal("Keine geschützten Branches"),
        ProtectionMessage::EmptyText => text.literal(
            "Fügen Sie eine Regel hinzu, damit ein Branch wie main nicht per Force-Push überschrieben, gelöscht oder ohne Pull-Request geändert wird.",
        ),
        ProtectionMessage::NoMatchYet => text.literal("Entspricht noch keinem Branch."),
        ProtectionMessage::MatchesLabel => text.literal("Treffer:"),
        ProtectionMessage::NewRule => text.literal("Neue Branch-Schutzregel"),
        ProtectionMessage::NewRuleIntro => text.literal(
            "Die Regel gilt für jeden Branch, dem ihr Muster entspricht, jetzt und künftig.",
        ),
        ProtectionMessage::Saved => text.literal("Regel gespeichert."),
        ProtectionMessage::Rule => text.literal("Regel"),
        ProtectionMessage::Pattern => text.literal("Muster für Branch-Namen"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("Ein Branch-Name oder ein Muster:"),
        ProtectionMessage::PatternOne => text.literal("passt innerhalb eines Namensteils,"),
        ProtectionMessage::PatternAcross => text.literal("über Teile hinweg ("),
        ProtectionMessage::PatternChar => text.literal("), "),
        ProtectionMessage::PatternTail => text.literal(
            "auf ein Zeichen. Groß- und Kleinschreibung werden unterschieden.",
        ),
        ProtectionMessage::Protect => text.literal("Passende Branches schützen"),
        ProtectionMessage::BlockForcePush => text.literal("Force-Pushes blockieren"),
        ProtectionMessage::BlockForcePushText => text.literal(
            "Pushes dürfen nur Commits hinzufügen; der Verlauf kann nicht umgeschrieben werden.",
        ),
        ProtectionMessage::BlockDeletion => text.literal("Löschen blockieren"),
        ProtectionMessage::BlockDeletionText => text.literal("Passende Branches können nicht gelöscht werden."),
        ProtectionMessage::RequirePullRequest => text.literal("Pull-Request erforderlich"),
        ProtectionMessage::RequirePullRequestText => text.literal(
            "Änderungen gelangen nur durch das Zusammenführen eines Pull-Requests hinein; direkte Pushes auf einen bestehenden Branch werden abgelehnt.",
        ),
        ProtectionMessage::RequiredApprovals => text.literal("Erforderliche Genehmigungen"),
        ProtectionMessage::DismissStale => text.literal(
            "Genehmigungen verwerfen, wenn neue Commits gepusht werden",
        ),
        ProtectionMessage::BlockOnChanges => text.literal(
            "Zusammenführen blockieren, solange Änderungen angefordert sind",
        ),
        ProtectionMessage::RestrictPushers => text.literal("Einschränken, wer pushen darf"),
        ProtectionMessage::RestrictPeople => text.literal(
            "Nur die folgenden Personen dürfen pushen oder zusammenführen. Deploy-Schlüssel dürfen es nie.",
        ),
        ProtectionMessage::RestrictPeopleTeams => text.literal(
            "Nur die folgenden Personen und Teams dürfen pushen oder zusammenführen. Deploy-Schlüssel dürfen es nie.",
        ),
        ProtectionMessage::People => text.literal("Personen"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal(
            "Benutzernamen, durch Leerzeichen oder Kommas getrennt. Jede Person benötigt Schreibzugriff.",
        ),
        ProtectionMessage::Teams => text.literal("Teams"),
        ProtectionMessage::NoTeams => text.literal("Diese Organisation hat noch keine Teams."),
        ProtectionMessage::RequireChecks => text.literal("Erfolgreiche Statusprüfungen erforderlich"),
        ProtectionMessage::RequireChecksText => text.literal(
            "Pull-Requests werden nur zusammengeführt, wenn der neueste Status jeder folgenden Prüfung auf ihrem Head-Commit erfolgreich ist.",
        ),
        ProtectionMessage::RequiredChecks => text.literal("Erforderliche Prüfungen"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("Administratoren die Umgehung erlauben"),
        ProtectionMessage::AdminBypassText => text.literal(
            "Repository-Administratoren sind an diese Regel nicht gebunden. Jede Umgehung wird im Audit-Log festgehalten.",
        ),
        ProtectionMessage::SaveRule => text.literal("Regel speichern"),
        ProtectionMessage::DangerZone => text.literal("Gefahrenbereich"),
        ProtectionMessage::DeleteTitle => text.literal("Diese Regel löschen"),
        ProtectionMessage::DeleteLead => text.literal("Branches, die"),
        ProtectionMessage::DeleteTail => text.literal("entsprechen, verlieren sofort ihren Schutz."),
        ProtectionMessage::DeleteConfirm => text.literal("Mir ist bewusst, dass diese Regel gelöscht wird"),
        ProtectionMessage::DeleteRule => text.literal("Regel löschen"),
        ProtectionMessage::NoMatches => text.literal("Entspricht noch keinem Branch"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("Dieses Repository hat die Höchstzahl von ");
            text.number(max);
            text.literal(" Regeln erreicht. Löschen Sie eine, um eine weitere hinzuzufügen.");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal(
                "Ein Kontext pro Zeile, genau so, wie Ihr CI ihn meldet (Groß- und Kleinschreibung beachten), höchstens ",
            );
            text.number(max);
            text.literal(".");
        }
        ProtectionText::Matches(names) => {
            text.literal("Entspricht ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("Entspricht ");
            text.argument(names);
            text.literal(" und ");
            text.number(more);
            text.literal(" weiteren");
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("Branch-Schutz · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("Neue Branch-Schutzregel · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · Branch-Schutz · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("Keine Force-Pushes"),
        RuleBadge::NoDeletion => text.literal("Kein Löschen"),
        RuleBadge::PullRequest => text.literal("Pull-Request"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Genehmigung",
                _ => " Genehmigungen",
            });
        }
        RuleBadge::StatusChecks => text.literal("Statusprüfungen"),
        RuleBadge::Restricted => text.literal("Eingeschränkt"),
        RuleBadge::AdminsBypass => text.literal("Admins dürfen umgehen"),
    }
}
