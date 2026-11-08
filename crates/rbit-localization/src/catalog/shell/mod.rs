//! Text of the pages around the repositories: signing in and up, account recovery,
//! the front page and dashboards, the notification inbox, error pages and the shared
//! page macros. English only so far: every other locale falls back (see
//! [`super::fallback`]), and templates mark that text through `ui::localized`.
//!
//! Each family lives in its own module; import it from there.
pub mod auth;
pub mod home;
pub mod inbox;
pub mod pages;
pub mod recovery;

use crate::catalog::fallback::MessageWriter;

/// Wording, one argument, wording.
fn phrase(text: &mut MessageWriter, before: &str, argument: &str, after: &str) {
    if !before.is_empty() {
        text.literal(before);
    }
    text.argument(argument);
    if !after.is_empty() {
        text.literal(after);
    }
}
