//! Russian (`ru`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Ru:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("Настройки"),
        AccessMessage::Collaborators => text.literal("Соавторы"),
        AccessMessage::IntroLead => text.literal("Люди, добавленные здесь, получают доступ к "),
        AccessMessage::IntroDirectly => text.literal(" напрямую"),
        AccessMessage::IntroOrganization => text.literal(", независимо от того, состоят ли они в организации"),
        AccessMessage::IntroTail => text.literal("."),
        AccessMessage::AddCollaborator => text.literal("Добавить соавтора"),
        AccessMessage::Username => text.literal("Имя пользователя"),
        AccessMessage::Permission => text.literal("Права"),
        AccessMessage::Add => text.literal("Добавить"),
        AccessMessage::People => text.literal("Люди"),
        AccessMessage::NoCollaborators => text.literal("Соавторов нет"),
        AccessMessage::NoCollaboratorsText => text.literal("Добавьте человека по имени пользователя, чтобы дать ему доступ к этому репозиторию."),
        AccessMessage::You => text.literal("(вы)"),
        AccessMessage::Save => text.literal("Сохранить"),
        AccessMessage::Leave => text.literal("Покинуть"),
        AccessMessage::Remove => text.literal("Удалить"),
        AccessMessage::Teams => text.literal("Команды"),
        AccessMessage::ManageTeams => text.literal("Управление командами"),
        AccessMessage::NoTeams => text.literal("Команд нет"),
        AccessMessage::NoTeamsText => text.literal("Откройте доступ команде на её странице; тогда её участники получат здесь эти права."),
        AccessMessage::Transfer => text.literal("Передача владения"),
        AccessMessage::TransferLead => text.literal("Передать"),
        AccessMessage::TransferTail => text.literal("другой учётной записи. Его история, соавторы и звёзды перейдут вместе с ним; ссылки и URL для клонирования со старым адресом перестанут работать, а команды текущей организации потеряют доступ."),
        AccessMessage::NewOwner => text.literal("Новый владелец"),
        AccessMessage::AdminEntry => text.literal("Как администратор сайта вы можете указать любого пользователя или организацию."),
        AccessMessage::Organization => text.literal("Организация"),
        AccessMessage::OwnerHint => text.literal("Ваша учётная запись или организация, в которой вы состоите."),
        AccessMessage::TypeLead => text.literal("Введите"),
        AccessMessage::TypeTail => text.literal("для подтверждения"),
        AccessMessage::TransferRepository => text.literal("Передать репозиторий"),
        AccessMessage::Nowhere => text.literal("Передать некому"),
        AccessMessage::NowhereText => text.literal("Вы не состоите ни в одной организации, где могли бы создавать репозитории, а этот репозиторий уже принадлежит вам."),
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.literal("Права для ");
            text.argument(name);
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("Сохранить права для ");
            text.argument(name);
        }
        AccessText::RemoveLabel(name) => {
            text.literal("Удалить ");
            text.argument(name);
        }
        AccessText::Added(date) => {
            text.literal("Добавлен ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("Соавторы · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("Передача · ");
            text.argument(repository);
        }
    }
}
