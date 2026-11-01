//! Trusted compiled first-party presentation, never external-package authority.
mod logs;
mod summary;
pub use logs::{LogLine, StepLog, StepLogBody};
use rbit_ui::i18n;
pub use summary::SummaryContent;
