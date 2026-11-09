//! Russian (`ru`) entries for `catalog/collab/sidebar.rs`.

use crate::catalog::collab::sidebar::IssueSidebarMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    IssueSidebarMessage => MessageWriter |text| {
        IssueSidebarMessage::Assignees => text.literal("Исполнители"),
        IssueSidebarMessage::NoOne => text.literal("Никого"),
        IssueSidebarMessage::AssignYourself => text.literal("назначить себя"),
        IssueSidebarMessage::EditAssignees => text.literal("Изменить исполнителей"),
        IssueSidebarMessage::SaveAssignees => text.literal("Сохранить исполнителей"),
        IssueSidebarMessage::Labels => text.literal("Метки"),
        IssueSidebarMessage::NoneYet => text.literal("Пока нет"),
        IssueSidebarMessage::EditLabels => text.literal("Изменить метки"),
        IssueSidebarMessage::NoRepositoryLabels => text.literal("В этом репозитории пока нет меток."),
        IssueSidebarMessage::SaveLabels => text.literal("Сохранить метки"),
        IssueSidebarMessage::Milestone => text.literal("Этап"),
        IssueSidebarMessage::NoMilestone => text.literal("Без этапа"),
        IssueSidebarMessage::ChangeMilestone => text.literal("Изменить этап"),
        IssueSidebarMessage::SaveMilestone => text.literal("Сохранить этап"),
        IssueSidebarMessage::Notifications => text.literal("Уведомления"),
        IssueSidebarMessage::Subscribe => text.literal("Подписаться"),
        IssueSidebarMessage::Unsubscribe => text.literal("Отписаться"),
        IssueSidebarMessage::NotNotifiedIssue => text.literal("Вы не получаете уведомления об этой задаче."),
        IssueSidebarMessage::NotNotifiedPull => text.literal("Вы не получаете уведомления об этом запросе на слияние."),
        IssueSidebarMessage::Danger => text.literal("Опасная зона"),
        IssueSidebarMessage::DeleteIssue => text.literal("Удалить задачу"),
        IssueSidebarMessage::DeletePull => text.literal("Удалить запрос на слияние"),
        IssueSidebarMessage::DeleteIssueConfirm => text.literal("Удалить эту задачу навсегда? Это действие нельзя отменить."),
        IssueSidebarMessage::DeletePullConfirm => text.literal("Удалить этот запрос на слияние навсегда? Это действие нельзя отменить."),
        IssueSidebarMessage::ReasonManual => text.literal("Вы подписались на это обсуждение."),
        IssueSidebarMessage::ReasonComment => text.literal("Вы комментировали это обсуждение."),
        IssueSidebarMessage::ReasonReviewed => text.literal("Вы оставили ревью к этому запросу на слияние."),
        IssueSidebarMessage::ReasonAuthor => text.literal("Вы открыли это обсуждение."),
        IssueSidebarMessage::ReasonMention => text.literal("Вас упомянули."),
        IssueSidebarMessage::ReasonAssigned => text.literal("Вы назначены исполнителем."),
        IssueSidebarMessage::ReasonReviewRequested => text.literal("У вас запросили ревью."),
    }
}
