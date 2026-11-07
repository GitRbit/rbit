//! Feature-owned inbox/read policy. Fanout, mail and background lifecycle remain open.
pub mod application;
mod models;
mod page;
pub mod port;
mod reason;
pub mod signals;
pub use models::{Notification, NotificationRecipient, Subscription};
pub use page::{InboxPage, NOTIFICATIONS_PER_PAGE};
pub use reason::NotificationReason;
