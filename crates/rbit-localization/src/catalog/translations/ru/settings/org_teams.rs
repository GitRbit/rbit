//! Russian (`ru`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};
use crate::plural::Plural;

entries! { Ru:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("Новая команда"),
        TeamMessage::NoTeams => text.literal("Команд пока нет"),
        TeamMessage::NoTeamsText => text.literal("Команды объединяют участников в группы и дают каждой группе свой доступ к репозиториям."),
        TeamMessage::OneMember => text.literal("1 участник"),
        TeamMessage::OneRepository => text.literal("1 репозиторий"),
        TeamMessage::NameHintLead => text.literal("Буквы, цифры,"),
        TeamMessage::NameHintAnd => text.literal("и"),
        TeamMessage::NameHintTail => text.literal("; используется в адресе команды."),
        TeamMessage::CreateTeam => text.literal("Создать команду"),
        TeamMessage::TeamSettings => text.literal("Настройки команды"),
        TeamMessage::RenameHint => text.literal("Переименование меняет адрес команды."),
        TeamMessage::SaveChanges => text.literal("Сохранить изменения"),
        TeamMessage::DeleteTeam => text.literal("Удалить эту команду"),
        TeamMessage::DeleteTeamText => text.literal("Её участники останутся в организации, но потеряют доступ, который давала эта команда."),
        TeamMessage::DeleteTeamButton => text.literal("Удалить команду"),
        TeamMessage::UsernamePlaceholder => text.literal("Имя пользователя"),
        TeamMessage::OnlyMembers => text.literal("Вступать в команды организации могут только её участники."),
        TeamMessage::NoMembers => text.literal("Участников нет"),
        TeamMessage::NoMembersText => text.literal("Участники получают доступ этой команды к её репозиториям."),
        TeamMessage::AddRepository => text.literal("Добавить репозиторий"),
        TeamMessage::RepositoryPlaceholder => text.literal("Имя репозитория"),
        TeamMessage::Permission => text.literal("Права"),
        TeamMessage::RepositoryHint => text.literal("Любой репозиторий организации. Участники команды получат на него эти права."),
        TeamMessage::NoRepositories => text.literal("Репозиториев нет"),
        TeamMessage::NoRepositoriesText => text.literal("Этой команде не открыт доступ ни к одному из видимых вам репозиториев."),
        TeamMessage::Private => text.literal("Приватный"),
        TeamMessage::Save => text.literal("Сохранить"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "участник",
                Plural::Few => "участника",
                _ => "участников",
            });
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "репозиторий",
                Plural::Few => "репозитория",
                _ => "репозиториев",
            });
        }
        TeamText::PermissionOn(repository) => {
            text.literal("Права на ");
            text.argument(repository);
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("Сохранить права на ");
            text.argument(repository);
        }
        TeamText::TeamsTitle(org) => {
            text.literal("Команды · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("Новая команда · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("Настройки · ");
            text.argument(team);
        }
    }
}
