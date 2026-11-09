//! Feature-owned inbox, subscription, fan-out and retention policy behind static ports.
//! Concrete stores keep SQL authority; mail and background lifecycle stay with composition.
mod activity;
pub mod application;
pub mod fanout;
mod models;
mod page;
pub mod port;
mod reason;
pub mod retention;
pub mod signals;
pub mod subscriptions;
pub use activity::{Activity, ActivityEvent, MentionedAccount, ThreadText};
pub use fanout::NOTIFICATION_BATCH_SIZE;
pub use models::{Notification, NotificationRecipient, Subscription};
pub use page::{InboxPage, NOTIFICATIONS_PER_PAGE};
pub use reason::NotificationReason;
