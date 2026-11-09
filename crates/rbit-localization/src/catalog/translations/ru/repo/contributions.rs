//! Russian (`ru`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};
use crate::plural::Plural;

entries! { Ru:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("Активность коммитов"),
        ContributionLabel::Loading => text.literal("Загрузка активности коммитов…"),
        ContributionLabel::ViewCommitActivity => text.literal("Показать активность коммитов"),
        ContributionLabel::Year => text.literal("Год"),
        ContributionLabel::View => text.literal("Вид"),
        ContributionLabel::AnnualCalendar => text.literal("Календарь коммитов за год"),
        ContributionLabel::QuarterlyCalendar => text.literal("Календарь коммитов по кварталам"),
        ContributionLabel::Monday => text.literal("Пн"),
        ContributionLabel::Wednesday => text.literal("Ср"),
        ContributionLabel::Friday => text.literal("Пт"),
        ContributionLabel::DatesUseUtc => text.literal("Даты указаны в UTC."),
        ContributionLabel::IntensityLegend => text.literal("Интенсивность коммитов от меньшей к большей"),
        ContributionLabel::Less => text.literal("Меньше"),
        ContributionLabel::More => text.literal("Больше"),
        ContributionLabel::Partial => text.literal("Активность неполная: как минимум в одном репозитории история превысила предел просмотра в 100 000 коммитов."),
        ContributionLabel::NoCommits => text.literal("Подходящих коммитов за этот год нет."),
        ContributionLabel::SelectDate => text.literal("Выберите дату, чтобы увидеть активность коммитов за этот день."),
        ContributionLabel::Commits => text.literal("Коммиты"),
        ContributionLabel::ActiveDays => text.literal("Активные дни"),
        ContributionLabel::LongestStreak => text.literal("Самая длинная серия"),
        ContributionLabel::Repositories => text.literal("Репозитории"),
        ContributionLabel::HowCounted => text.literal("Активность по месяцам и правила подсчёта коммитов"),
        ContributionLabel::HowCountedText => text.literal("Учитываются коммиты, достижимые из настроенной ветки по умолчанию, по дате их автора. Одинаковые коммиты в разных репозиториях считаются один раз. Учитываются только репозитории, к которым у вас есть доступ. Задачи, запросы на слияние и ревью не учитываются."),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
            text.literal(" в ");
            text.argument(&year.to_string());
            text.literal(" году");
        }
        ContributionMessage::ActivityInYear { year } => {
            text.literal("Активность коммитов за ");
            text.argument(&year.to_string());
            text.literal(" год");
        }
        ContributionMessage::Description { count, days } => {
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
            text.literal(" за ");
            text.number(days as u64);
            text.literal(" ");
            text.literal(match text.plural(days as u64) {
                Plural::One => "активный день",
                Plural::Few => "активных дня",
                _ => "активных дней",
            });
            text.literal(". Чем насыщеннее цвет, тем больше коммитов. Наведите указатель или фокус на день, чтобы увидеть число коммитов. Выберите день, чтобы увидеть подробности.");
        }
        ContributionMessage::Day { count, date } => {
            text.argument(date);
            text.literal(": ");
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "коммит",
                Plural::Few => "коммита",
                _ => "коммитов",
            });
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.literal("Активность коммитов за ");
            text.argument(quarter);
            text.literal(" ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(" ");
            text.literal(match text.plural(count as u64) {
                Plural::One => "день",
                Plural::Few => "дня",
                _ => "дней",
            });
        }
    }
}
