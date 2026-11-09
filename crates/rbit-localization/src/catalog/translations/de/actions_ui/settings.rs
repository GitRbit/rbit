//! German (`de`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.literal("Workflows unter ");
            text.argument(directory);
            text.literal(" laufen auf Runnern, wenn ihre Ereignisse in ");
            text.argument(repository);
            text.literal(" eintreten.");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("Secrets und Variablen für die Workflows der Repositorys von ");
            text.argument(organization);
            text.literal(". Ein gleichnamiges Secret oder eine gleichnamige Variable des Repositorys hat Vorrang.");
        }
        ActionsSettingsMessage::AccountIntro => text.literal(
            "Secrets und Variablen für die Workflows Ihrer eigenen Repositorys. Ein gleichnamiges Secret oder eine \
             gleichnamige Variable des Repositorys hat Vorrang.",
        ),
        ActionsSettingsMessage::General => text.literal("Allgemein"),
        ActionsSettingsMessage::RunWorkflows => text.literal("Workflows in diesem Repository ausführen"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal(
            "Aus: Es starten keine neuen Ausführungen und der Actions-Tab ist ausgeblendet. Frühere Ausführungen und \
             ihre Protokolle bleiben erhalten.",
        ),
        ActionsSettingsMessage::DefaultPermission => text.literal("Standardberechtigung des Workflow-Tokens"),
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("Der Schlüssel ");
            text.argument(key);
            text.literal(
                " eines Workflows kann sie nur senken. Ausführungen aus Forks und von externen Autoren erhalten immer \
                 ein schreibgeschütztes Token.",
            );
        }
        ActionsSettingsMessage::PermissionRead => text.literal("Lesen"),
        ActionsSettingsMessage::PermissionReadHint => {
            text.literal("Workflows können Inhalte, Issues und Pull-Requests des Repositorys lesen.");
        }
        ActionsSettingsMessage::PermissionWrite => text.literal("Lesen und schreiben"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal(
            "Workflows können außerdem pushen sowie Issues und Pull-Requests kommentieren. Der Branch-Schutz gilt \
             weiterhin.",
        ),
        ActionsSettingsMessage::SaveChanges => text.literal("Änderungen speichern"),
        ActionsSettingsMessage::RepositoryRunnersHint => {
            text.literal("Die Runner dieses Repositorys, die geerbten Runner und Registrierungstokens.");
        }
        ActionsSettingsMessage::OrganizationRunnersHint => {
            text.literal("Runner für alle Repositorys der Organisation und Registrierungstokens.");
        }
        ActionsSettingsMessage::AccountRunnersHint => {
            text.literal("Runner für alle Repositorys Ihres Kontos und Registrierungstokens.");
        }
        ActionsSettingsMessage::EnvironmentsHint => text.literal(
            "Deployment-Umgebungen: erforderliche Reviewer, erlaubte Branches sowie eigene Secrets und Variablen.",
        ),
        ActionsSettingsMessage::TrustedTitle => text.literal("Vertrauenswürdige Pull-Request-Autoren"),
        ActionsSettingsMessage::TrustedIntro => text.literal(
            "Ihre Pull-Requests aus Forks laufen, ohne auf eine Genehmigung zu warten, weiterhin mit einem \
             schreibgeschützten Token und ohne Secrets oder Variablen. Autoren werden bei der Genehmigung einer \
             Ausführung mit „Pull-Requests dieses Autors immer ohne Genehmigung ausführen“ hinzugefügt.",
        ),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("Keine vertrauenswürdigen Autoren"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal(
            "Pull-Requests aus Forks und von Personen ohne Schreibzugriff warten darauf, dass jemand mit \
             Schreibzugriff jede Ausführung genehmigt.",
        ),
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("Vertraut seit ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("Vertraut seit ");
            text.argument(since);
            text.literal(" durch ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.argument(name);
            text.literal(" nicht mehr vertrauen");
        }
        ActionsSettingsMessage::Remove => text.literal("Entfernen"),
        ActionsSettingsMessage::Saved => text.literal("Einstellungen gespeichert."),
        ActionsSettingsMessage::SecretSaved => text.literal("Secret gespeichert."),
        ActionsSettingsMessage::SecretDeleted => text.literal("Secret gelöscht."),
        ActionsSettingsMessage::VariableSaved => text.literal("Variable gespeichert."),
        ActionsSettingsMessage::VariableDeleted => text.literal("Variable gelöscht."),
        ActionsSettingsMessage::Untrusted => {
            text.literal("Die Pull-Requests dieses Autors müssen wieder genehmigt werden.");
        }
        ActionsSettingsMessage::RepositoryAccess => text.literal("Repository-Zugriff"),
        ActionsSettingsMessage::AllRepositories => text.literal("Alle Repositorys"),
        ActionsSettingsMessage::AllRepositoriesHint => {
            text.literal("Alle Repositorys dieses Inhabers, auch später erstellte.");
        }
        ActionsSettingsMessage::SelectedRepositories => text.literal("Ausgewählte Repositorys"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("Nur die unten markierten Repositorys."),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("Dieser Inhaber hat noch keine Repositorys."),
    }
}
