//! Russian (`ru`) entries for `catalog/collab/filters.rs`.

use crate::catalog::collab::filters::IssueFilterMenu;
use crate::catalog::collab::filters::IssueFilterMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    IssueFilterMessage => MessageWriter |text| {
        IssueFilterMessage::Menu(IssueFilterMenu::State) => text.literal("Состояние"),
        IssueFilterMessage::Menu(IssueFilterMenu::Status) => text.literal("Статус"),
        IssueFilterMessage::Menu(IssueFilterMenu::Author) => text.literal("Автор"),
        IssueFilterMessage::Menu(IssueFilterMenu::Label) => text.literal("Метка"),
        IssueFilterMessage::Menu(IssueFilterMenu::Milestone) => text.literal("Этап"),
        IssueFilterMessage::Menu(IssueFilterMenu::Assignee) => text.literal("Исполнитель"),
        IssueFilterMessage::Menu(IssueFilterMenu::Sort) => text.literal("Сортировка"),
        IssueFilterMessage::Find(IssueFilterMenu::State) => text.literal("Поиск состояния"),
        IssueFilterMessage::Find(IssueFilterMenu::Status) => text.literal("Поиск статуса"),
        IssueFilterMessage::Find(IssueFilterMenu::Author) => text.literal("Поиск автора"),
        IssueFilterMessage::Find(IssueFilterMenu::Label) => text.literal("Поиск метки"),
        IssueFilterMessage::Find(IssueFilterMenu::Milestone) => text.literal("Поиск этапа"),
        IssueFilterMessage::Find(IssueFilterMenu::Assignee) => text.literal("Поиск исполнителя"),
        IssueFilterMessage::Find(IssueFilterMenu::Sort) => text.literal("Поиск сортировки"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::State) => text.literal("Найти состояние…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Status) => text.literal("Найти статус…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Author) => text.literal("Найти автора…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Label) => text.literal("Найти метку…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Milestone) => text.literal("Найти этап…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Assignee) => text.literal("Найти исполнителя…"),
        IssueFilterMessage::FindPlaceholder(IssueFilterMenu::Sort) => text.literal("Найти сортировку…"),
        IssueFilterMessage::NothingToChoose => text.literal("Выбирать пока не из чего."),
        IssueFilterMessage::NoMatches => text.literal("Совпадений нет."),
        IssueFilterMessage::Anyone => text.literal("Любой"),
        IssueFilterMessage::Nobody => text.literal("Никто"),
        IssueFilterMessage::You => text.literal("Вы"),
        IssueFilterMessage::Any => text.literal("Любой"),
        IssueFilterMessage::NoMilestone => text.literal("Без этапа"),
        IssueFilterMessage::AnyLabel => text.literal("Любая метка"),
        IssueFilterMessage::NoLabels => text.literal("Без меток"),
        IssueFilterMessage::Open => text.literal("Открытые"),
        IssueFilterMessage::Closed => text.literal("Закрытые"),
        IssueFilterMessage::AllStates => text.literal("Все состояния"),
        IssueFilterMessage::AnyStatus => text.literal("Любой статус"),
        IssueFilterMessage::Draft => text.literal("Черновик"),
        IssueFilterMessage::Merged => text.literal("Слитые"),
        IssueFilterMessage::SortNewest => text.literal("Сначала новые"),
        IssueFilterMessage::SortOldest => text.literal("Сначала старые"),
        IssueFilterMessage::SortRecentlyUpdated => text.literal("Недавно обновлённые"),
        IssueFilterMessage::SortLeastRecentlyUpdated => text.literal("Давно не обновлявшиеся"),
        IssueFilterMessage::SortMostCommented => text.literal("Больше всего комментариев"),
    }
}
