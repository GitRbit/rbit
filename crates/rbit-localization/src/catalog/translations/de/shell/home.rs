//! German (`de`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};
use crate::plural::Plural;

entries! { De:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("Öffentliche Repositorys durchsuchen"),
        HomeMessage::Search => text.literal("Suchen"),
        HomeMessage::ExploreAll => text.literal("Alle Repositorys entdecken"),
        HomeMessage::ThisInstance => text.literal("Diese Instanz"),
        HomeMessage::RepositoryCount(count) => text.literal(match text.plural(count.unsigned_abs()) {
            Plural::One => "Repository",
            _ => "Repositorys",
        }),
        HomeMessage::UserCount(_) => text.literal("Benutzer"),
        HomeMessage::OrganizationCount(count) => text.literal(match text.plural(count.unsigned_abs()) {
            Plural::One => "Organisation",
            _ => "Organisationen",
        }),
        HomeMessage::RecentlyUpdated => text.literal("Kürzlich aktualisiert"),
        HomeMessage::ViewAll => text.literal("Alle anzeigen"),
        HomeMessage::NoPublicRepositories => text.literal("Noch keine öffentlichen Repositorys"),
        HomeMessage::NoPublicRepositoriesText => text.literal(
            "Repositorys, die auf diesem Server veröffentlicht werden, erscheinen hier.",
        ),
        HomeMessage::FreshServer => text.literal(
            "Dieser Server hat noch keine Konten. Das erste registrierte Konto wird sein Administrator.",
        ),
        HomeMessage::BuiltForSelfHosting => text.literal("Für das Selbsthosten gebaut"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "Eine schnelle Binärdatei",
            Feature::Protocols => "Git über HTTPS und SSH",
            Feature::Teams => "Organisationen und Teams",
            Feature::Releases => "Releases, Spiegel und Webhooks",
            Feature::Security => "Standardmäßig sicher",
            Feature::Screens => "Auf jedem Bildschirm",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite und ein eingebetteter Speicher. Keine weiteren Dienste zu installieren oder zu betreiben.",
            Feature::Protocols => "Smart HTTP mit Protokoll v2, ein integrierter SSH-Server und Deploy-Schlüssel.",
            Feature::Teams => "Teilen Sie Repositorys mit Teams und Mitarbeitenden, Rolle für Rolle.",
            Feature::Releases => "Veröffentlichen Sie Releases, spiegeln Sie Upstream-Projekte und benachrichtigen Sie andere Dienste.",
            Feature::Security => "Zwei-Faktor-Anmeldung, Zugriffstokens mit Bereichen und ein Audit-Log.",
            Feature::Screens => "Responsiv bis zur Smartphone-Größe und als App installierbar.",
        }),
        HomeMessage::Dashboard => text.literal("Dashboard"),
        HomeMessage::ViewProfile => text.literal("Profil anzeigen"),
        HomeMessage::New => text.literal("Neu"),
        HomeMessage::NewRepository => text.literal("Neues Repository"),
        HomeMessage::PersonalDashboards => text.literal("Persönliche Dashboards"),
        HomeMessage::Issues => text.literal("Issues"),
        HomeMessage::PullRequests => text.literal("Pull-Requests"),
        HomeMessage::Repositories => text.literal("Repositorys"),
        HomeMessage::PrivateCount(count) => {
            text.number(u64::try_from(count).unwrap_or(u64::MAX));
            text.literal(" privat");
        }
        HomeMessage::FindRepository => text.literal("Repository suchen…"),
        HomeMessage::FilterRepositories => text.literal("Ihre Repositorys filtern"),
        HomeMessage::NoRepositories => text.literal("Noch keine Repositorys"),
        HomeMessage::NoRepositoriesText => text.literal(
            "Erstellen Sie ein Repository, um Code auf diesem Server zu hosten.",
        ),
        HomeMessage::CreateRepository => text.literal("Repository erstellen"),
        HomeMessage::NoFilterMatch => text.literal("Keine Repositorys entsprechen Ihrem Filter."),
        HomeMessage::Organizations => text.literal("Organisationen"),
        HomeMessage::NewOrganization => text.literal("Neue Organisation"),
        HomeMessage::NotAMember => text.literal("Sie sind kein Mitglied einer Organisation."),
        HomeMessage::CreateOne => text.literal("Erstellen Sie eine Organisation"),
        HomeMessage::ShareWithTeam => text.literal("und teilen Sie Repositorys mit einem Team."),
        HomeMessage::Explore => text.literal("Entdecken"),
        HomeMessage::NothingPublic => text.literal("Noch nichts Öffentliches."),
        HomeMessage::YourInvolvement => text.literal("Ihre Beteiligung"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "Von mir erstellt",
            Involvement::Assigned => "Zugewiesen",
            Involvement::Mentioned => "Erwähnt",
            Involvement::ReviewRequested => "Review angefordert",
        }),
        HomeMessage::SearchIn(title) => {
            text.argument(title);
            text.literal(" durchsuchen");
        }
        HomeMessage::SearchHelp => text.literal("Suchhilfe"),
        HomeMessage::SearchHelpText => text.literal(
            "Verwenden Sie is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee \
             oder sort:updated-desc. Freitext durchsucht Titel und Beschreibungen. Die Ergebnisse umfassen nur \
             Repositorys, die Sie derzeit lesen können. „Erwähnt“ umfasst bekannte Erwähnungen in Beschreibungen, \
             Kommentaren und abgeschickten Reviews, auch nachdem die Benachrichtigungen gelesen wurden.",
        ),
        HomeMessage::Results => text.literal("Ergebnisse"),
        HomeMessage::OpenCount(count) => {
            text.number(count);
            text.literal(" offen");
        }
        HomeMessage::ClosedCount(count) => {
            text.number(count);
            text.literal(" geschlossen");
        }
        HomeMessage::NoMatchingThreads => text.literal("Keine passenden Threads"),
        HomeMessage::NoMatchingThreadsText => text.literal(
            "Versuchen Sie einen anderen Filter oder Beteiligungs-Tab.",
        ),
        HomeMessage::CommentCount(count) => {
            text.number(u64::from(count));
            text.literal(match text.plural(u64::from(count)) {
                Plural::One => " Kommentar",
                _ => " Kommentare",
            });
        }
    }
}
