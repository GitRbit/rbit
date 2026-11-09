//! Feature-owned inbox, subscription, fan-out, retention, email and webhook delivery
//! policy behind static ports.
//! Concrete stores keep SQL authority; templates, transports and background lifecycle stay
//! with composition.
mod activity;
pub mod application;
pub mod fanout;
pub mod mail;
mod models;
mod page;
pub mod port;
mod reason;
pub mod retention;
pub mod signals;
pub mod subscriptions;
pub mod webhooks;
pub use activity::{Activity, ActivityEvent, MentionedAccount, ThreadText};
pub use fanout::NOTIFICATION_BATCH_SIZE;
pub use models::{Notification, NotificationRecipient, Subscription};
pub use page::{InboxPage, NOTIFICATIONS_PER_PAGE};
pub use reason::NotificationReason;
