//! Russian (`ru`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};
use crate::plural::Plural;

entries! { Ru:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("Поставьте флажок для подтверждения."),
        ProtectionMessage::Settings => text.literal("Настройки"),
        ProtectionMessage::Title => text.literal("Защита веток"),
        ProtectionMessage::IntroLead => text.literal("Правила применяются к каждой отправке в"),
        ProtectionMessage::IntroTail => text.literal(" по HTTP и SSH, в том числе для администраторов, если правило не позволяет им его обходить."),
        ProtectionMessage::AddRule => text.literal("Добавить правило"),
        ProtectionMessage::Deleted => text.literal("Правило удалено."),
        ProtectionMessage::Empty => text.literal("Защищённых веток нет"),
        ProtectionMessage::EmptyText => text.literal("Добавьте правило, чтобы защитить ветку, например main, от принудительной отправки, удаления и изменений без запроса на слияние."),
        ProtectionMessage::NoMatchYet => text.literal("Пока не соответствует ни одной ветке."),
        ProtectionMessage::MatchesLabel => text.literal("Соответствует:"),
        ProtectionMessage::NewRule => text.literal("Новое правило защиты ветки"),
        ProtectionMessage::NewRuleIntro => text.literal("Правило применяется ко всем веткам, подходящим под шаблон, — сейчас и в будущем."),
        ProtectionMessage::Saved => text.literal("Правило сохранено."),
        ProtectionMessage::Rule => text.literal("Правило"),
        ProtectionMessage::Pattern => text.literal("Шаблон имени ветки"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("Имя ветки или шаблон:"),
        ProtectionMessage::PatternOne => text.literal("— любые символы в пределах одной части имени,"),
        ProtectionMessage::PatternAcross => text.literal("— через несколько частей ("),
        ProtectionMessage::PatternChar => text.literal("), "),
        ProtectionMessage::PatternTail => text.literal("— один символ. Регистр учитывается."),
        ProtectionMessage::Protect => text.literal("Защитить подходящие ветки"),
        ProtectionMessage::BlockForcePush => text.literal("Запретить принудительную отправку"),
        ProtectionMessage::BlockForcePushText => text.literal("Отправка может только добавлять коммиты; переписать историю нельзя."),
        ProtectionMessage::BlockDeletion => text.literal("Запретить удаление"),
        ProtectionMessage::BlockDeletionText => text.literal("Подходящие ветки нельзя удалить."),
        ProtectionMessage::RequirePullRequest => text.literal("Требовать запрос на слияние"),
        ProtectionMessage::RequirePullRequestText => text.literal("Изменения попадают только через слияние запроса; прямая отправка в существующую ветку отклоняется."),
        ProtectionMessage::RequiredApprovals => text.literal("Обязательные одобрения"),
        ProtectionMessage::DismissStale => text.literal("Отзывать одобрения при отправке новых коммитов"),
        ProtectionMessage::BlockOnChanges => text.literal("Запрещать слияние, пока запрошены изменения"),
        ProtectionMessage::RestrictPushers => text.literal("Ограничить круг тех, кто может отправлять изменения"),
        ProtectionMessage::RestrictPeople => text.literal("Отправлять изменения и выполнять слияние могут только люди, указанные ниже. Ключам развёртывания это запрещено всегда."),
        ProtectionMessage::RestrictPeopleTeams => text.literal("Отправлять изменения и выполнять слияние могут только люди и команды, указанные ниже. Ключам развёртывания это запрещено всегда."),
        ProtectionMessage::People => text.literal("Люди"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal("Имена пользователей через пробел или запятую. Каждому нужно право записи."),
        ProtectionMessage::Teams => text.literal("Команды"),
        ProtectionMessage::NoTeams => text.literal("В этой организации пока нет команд."),
        ProtectionMessage::RequireChecks => text.literal("Требовать прохождения проверок статуса"),
        ProtectionMessage::RequireChecksText => text.literal("Запросы на слияние сливаются, только если последний статус каждой проверки ниже — успех на их последнем коммите."),
        ProtectionMessage::RequiredChecks => text.literal("Обязательные проверки"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("Разрешить администраторам обход"),
        ProtectionMessage::AdminBypassText => text.literal("Это правило не распространяется на администраторов репозитория. Каждый обход записывается в журнал аудита."),
        ProtectionMessage::SaveRule => text.literal("Сохранить правило"),
        ProtectionMessage::DangerZone => text.literal("Опасная зона"),
        ProtectionMessage::DeleteTitle => text.literal("Удалить это правило"),
        ProtectionMessage::DeleteLead => text.literal("Все ветки по шаблону"),
        ProtectionMessage::DeleteTail => text.literal("сразу потеряют защиту."),
        ProtectionMessage::DeleteConfirm => text.literal("Я понимаю, что это правило будет удалено"),
        ProtectionMessage::DeleteRule => text.literal("Удалить правило"),
        ProtectionMessage::NoMatches => text.literal("Пока не соответствует ни одной ветке"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("В этом репозитории уже максимальное число правил — ");
            text.number(max);
            text.literal(". Удалите одно, чтобы добавить другое.");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal("По одному контексту на строку, точно как его сообщает ваша CI (с учётом регистра), не более ");
            text.number(max);
            text.literal(".");
        }
        ProtectionText::Matches(names) => {
            text.literal("Соответствует: ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("Соответствует: ");
            text.argument(names);
            text.literal(" и ещё ");
            text.number(more);
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("Защита веток · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("Новое правило защиты ветки · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · Защита веток · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("Без принудительной отправки"),
        RuleBadge::NoDeletion => text.literal("Без удаления"),
        RuleBadge::PullRequest => text.literal("Запрос на слияние"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(" ");
            text.literal(match text.plural(count) {
                Plural::One => "одобрение",
                Plural::Few => "одобрения",
                _ => "одобрений",
            });
        }
        RuleBadge::StatusChecks => text.literal("Проверки статуса"),
        RuleBadge::Restricted => text.literal("Ограничено"),
        RuleBadge::AdminsBypass => text.literal("Обход для администраторов"),
    }
}
