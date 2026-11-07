//! Original inbox pagination semantics, including saturating page counts.
use crate::Notification;
/// One page of a user's inbox, rows of unreadable repositories already removed.
#[derive(Debug, Clone)]
pub struct InboxPage {
    pub notifications: Vec<Notification>,
    pub total: u64,
    /// 1-based.
    pub page: u32,
    pub per_page: u32,
}

impl InboxPage {
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.total.div_ceil(u64::from(self.per_page)).max(1)).unwrap_or(u32::MAX)
    }
}

/// Rows per default HTML inbox page.
pub const NOTIFICATIONS_PER_PAGE: u32 = 50;
