//! Repository milestone catalogs; attaching threads and publishing timelines stay separate.

mod models;
mod validation;

pub use models::{Milestone, MilestoneFields, MilestoneId, MilestoneInput, MilestoneProgress, MilestoneRef};
pub use validation::{MAX_MILESTONE_DESCRIPTION_BYTES, MAX_MILESTONE_TITLE_CHARS, is_calendar_day, milestone_fields};

#[cfg(feature = "applications")]
mod application;
#[cfg(feature = "applications")]
mod repository;
#[cfg(feature = "applications")]
pub use application::{MilestoneAccess, MilestoneApplication, MilestoneError};
#[cfg(feature = "applications")]
pub use repository::MilestoneRepository;
