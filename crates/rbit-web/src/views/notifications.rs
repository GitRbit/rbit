//! The notification inbox.
//!
//! Milestone 2 workstream I (issues) owns this file and `templates/notifications/**`.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::{IssueKind, IssueState, StateReason};
use rbit_services::notifications::Notification;

use crate::{
    urls::issue_url,
    views::{components::Pagination, layout::Page},
};

/// One thread in the inbox.
#[derive(Debug, Clone)]
pub struct NotificationRow {
    pub title: String,
    pub url: String,
    /// `#12`.
    pub number: String,
    pub icon: &'static str,
    /// A token utility for the icon.
    pub icon_class: &'static str,
    pub state_label: &'static str,
    pub reason: &'static str,
    pub when: String,
    pub when_iso: String,
    pub unread: bool,
    pub read_url: String,
    pub unsubscribe_url: String,
}

impl NotificationRow {
    pub fn new(notification: &Notification) -> Self {
        let (icon, icon_class, state_label) = match (notification.kind, notification.state, notification.state_reason) {
            (IssueKind::Issue, IssueState::Open, _) => ("circle-dot", "text-success", "Open issue"),
            (IssueKind::Issue, IssueState::Closed, Some(StateReason::NotPlanned)) => ("circle-slash", "text-ink-muted", "Closed issue"),
            (IssueKind::Issue, IssueState::Closed, _) => ("circle-check", "text-ink-muted", "Closed issue"),
            (IssueKind::Pull, IssueState::Open, _) => ("git-pull-request", "text-success", "Open pull request"),
            (IssueKind::Pull, IssueState::Closed, Some(StateReason::Merged)) => ("git-merge", "text-ink", "Merged pull request"),
            (IssueKind::Pull, IssueState::Closed, _) => ("git-pull-request-closed", "text-ink-muted", "Closed pull request"),
        };
        let base = format!("/notifications/{}", notification.id);
        Self {
            title: notification.title.clone(),
            url: issue_url(&notification.owner, &notification.repo_name, notification.kind, notification.number),
            number: notification.number.to_string(),
            icon,
            icon_class,
            state_label,
            reason: notification.reason.label(),
            when: notification.updated_at.relative(),
            when_iso: notification.updated_at.iso8601(),
            unread: notification.unread,
            read_url: format!("{base}/read"),
            unsubscribe_url: format!("{base}/unsubscribe"),
        }
    }
}

/// The threads of one repository.
#[derive(Debug, Clone)]
pub struct NotificationGroup {
    pub full_name: String,
    pub url: String,
    pub rows: Vec<NotificationRow>,
}

/// Groups `notifications` (newest first) by repository, keeping the order in which each
/// repository first appears.
pub fn group(notifications: &[Notification]) -> Vec<NotificationGroup> {
    let mut groups: Vec<NotificationGroup> = Vec::new();
    for notification in notifications {
        let full_name = notification.full_name();
        let row = NotificationRow::new(notification);
        match groups.iter_mut().find(|group| group.full_name == full_name) {
            Some(group) => group.rows.push(row),
            None => groups.push(NotificationGroup { url: format!("/{full_name}"), full_name, rows: vec![row] }),
        }
    }
    groups
}

/// `/notifications[?all=1]`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "notifications/inbox.html")]
pub struct InboxPage {
    pub page: Page,
    pub groups: Vec<NotificationGroup>,
    pub showing_all: bool,
    pub unread_url: String,
    pub all_url: String,
    pub mark_all_url: String,
    pub pagination: Pagination,
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use rbit_core::{AccountId, IssueId, IssueNumber, RepoId, Timestamp};
    use rbit_services::notifications::NotificationReason;

    use super::*;

    fn notification(id: i64, repo: &str, kind: IssueKind, unread: bool) -> Notification {
        Notification {
            id,
            user: AccountId(1),
            issue: IssueId(id),
            repo: RepoId(1),
            reason: NotificationReason::Mention,
            unread,
            last_actor: None,
            updated_at: Timestamp(0),
            number: IssueNumber::new(3).unwrap(),
            kind,
            state: IssueState::Open,
            state_reason: None,
            title: "Crash <x>".into(),
            owner: "alice".into(),
            repo_name: repo.into(),
        }
    }

    #[test]
    fn the_inbox_groups_by_repository() {
        let rows = [
            notification(1, "demo", IssueKind::Issue, true),
            notification(2, "web", IssueKind::Pull, false),
            notification(3, "demo", IssueKind::Issue, true),
        ];
        let groups = group(&rows);
        assert_eq!(groups.iter().map(|g| g.full_name.as_str()).collect::<Vec<_>>(), ["alice/demo", "alice/web"]);
        assert_eq!(groups[0].rows.len(), 2);
        assert_eq!(groups[1].rows[0].url, "/alice/web/pull/3");
        assert_eq!(groups[0].rows[0].read_url, "/notifications/1/read");

        let html = InboxPage {
            page: Page {
                locale: rbit_messages::LocaleSnapshot::english(0),
                has_i18n: false,
                updates: rbit_services::updates::UpdateStatus::default(),
                site_name: "rbit".into(),
                plugin_revision: 0,
                branding: rbit_core::config::BrandingConfig::default(),
                title: None,
                viewer: None,
                path: "/notifications".into(),
                has_unread_notifications: true,
                theme: crate::themes::Theme::System,
            },
            groups,
            showing_all: false,
            unread_url: "/notifications".into(),
            all_url: "/notifications?all=1".into(),
            mark_all_url: "/notifications/read-all".into(),
            pagination: Pagination::default(),
            total: 3,
        }
        .render()
        .unwrap();
        assert!(html.contains("alice/demo") && html.contains("Mentioned") && html.contains("Mark all as read"));
        assert!(!html.contains("<x>"), "titles are escaped");
        assert!(!html.contains("style="));
    }
}
