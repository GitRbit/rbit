//! Russian (`ru`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};
use crate::plural::Plural;

entries! { Ru:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("Выберите права."),
        OrgMessage::Organization => text.literal("Организация"),
        OrgMessage::Repositories => text.literal("Репозитории"),
        OrgMessage::Members => text.literal("Участники"),
        OrgMessage::Teams => text.literal("Команды"),
        OrgMessage::Settings => text.literal("Настройки"),
        OrgMessage::NewOrganization => text.literal("Новая организация"),
        OrgMessage::NewIntro => text.literal("Организация владеет репозиториями от имени команды. Вы станете её первым владельцем."),
        OrgMessage::Name => text.literal("Имя"),
        OrgMessage::NameHint => text.literal("Используется в URL. Буквы, цифры, «-», «_» и «.». У пользователей и организаций общее пространство имён."),
        OrgMessage::DisplayName => text.literal("Отображаемое имя"),
        OrgMessage::Optional => text.literal("необязательно"),
        OrgMessage::Description => text.literal("Описание"),
        OrgMessage::Cancel => text.literal("Отмена"),
        OrgMessage::CreateOrganization => text.literal("Создать организацию"),
        OrgMessage::New => text.literal("Создать"),
        OrgMessage::NoRepositories => text.literal("Репозиториев пока нет"),
        OrgMessage::NoRepositoriesText => text.literal("Здесь появятся репозитории, принадлежащие этой организации."),
        OrgMessage::Profile => text.literal("Профиль"),
        OrgMessage::Website => text.literal("Веб-сайт"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("Сохранить профиль"),
        OrgMessage::MemberPermission => text.literal("Права участников"),
        OrgMessage::PermissionLegend => text.literal("Что каждый участник получает в каждом репозитории"),
        OrgMessage::PermissionHint => text.literal("Команды и соавторы могут только расширить эти права, но не урезать. Владельцы всегда управляют всеми репозиториями."),
        OrgMessage::SavePermission => text.literal("Сохранить права"),
        OrgMessage::NoPermission => text.literal("Без прав"),
        OrgMessage::NoPermissionText => text.literal("Участники видят только публичные репозитории и те, к которым есть доступ у их команд."),
        OrgMessage::Read => text.literal("Чтение"),
        OrgMessage::ReadText => text.literal("Участники могут просматривать и клонировать все репозитории."),
        OrgMessage::Write => text.literal("Запись"),
        OrgMessage::WriteText => text.literal("Участники могут просматривать, клонировать все репозитории и отправлять в них изменения."),
        OrgMessage::DangerZone => text.literal("Опасная зона"),
        OrgMessage::DeleteOrganization => text.literal("Удалить эту организацию"),
        OrgMessage::DeleteText => text.literal("Организация и все членства в ней будут удалены навсегда. Это действие нельзя отменить."),
        OrgMessage::TypeToConfirmLead => text.literal("Введите"),
        OrgMessage::TypeToConfirmTail => text.literal("для подтверждения"),
        OrgMessage::DeleteOrganizationButton => text.literal("Удалить организацию"),
        OrgMessage::AddMember => text.literal("Добавить участника"),
        OrgMessage::Username => text.literal("Имя пользователя"),
        OrgMessage::Role => text.literal("Роль"),
        OrgMessage::Member => text.literal("Участник"),
        OrgMessage::Owner => text.literal("Владелец"),
        OrgMessage::Add => text.literal("Добавить"),
        OrgMessage::MembersHint => text.literal("Участники получают базовые права организации на каждый репозиторий, а также всё, что дают их команды. Владельцы также могут управлять участниками, командами и настройками."),
        OrgMessage::You => text.literal("(вы)"),
        OrgMessage::Leave => text.literal("Покинуть"),
        OrgMessage::Remove => text.literal("Удалить"),
        OrgMessage::MakeOwner => text.literal("Сделать владельцем"),
        OrgMessage::MakeMember => text.literal("Сделать участником"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" всё ещё владеет 1 репозиторием. Сначала удалите или передайте его.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" всё ещё владеет ");
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "репозиторием",
                _ => "репозиториями",
            });
            text.literal(". Сначала удалите или передайте их.");
        }
        OrgText::RemoveLabel(name) => {
            text.literal("Удалить ");
            text.argument(name);
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal(": ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("Участники · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("Настройки · ");
            text.argument(org);
        }
    }
}
