//! Repository pages: browsing, history, editing, settings, releases and contributions.
//!
//! English only so far: every other locale falls back (see [`super::fallback`]). Fixed
//! labels are `…Label` families, declared one line each through [`labels!`]; messages
//! with counts or names are hand-written `…Message` families. Both offer
//! `text(&locale)`, the lookup templates call as `Family::Key.text(page.locale)`.
/// Declares a family of fixed English labels: `Variant => "stable.id", "English";`.
macro_rules! labels {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $id:literal, $english:literal;)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            $(#[doc = $english] $variant,)+
        }

        impl $name {
            /// Every label of the family, for catalog checks.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        impl $crate::catalog::fallback::MessageFamily for $name {
            fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)+
                }
            }

            fn english(self, text: &mut $crate::catalog::fallback::MessageWriter) {
                text.literal(match self {
                    $(Self::$variant => $english,)+
                });
            }

            fn translate(self, locale: $crate::locale::Locale, text: &mut $crate::catalog::fallback::MessageWriter) -> bool {
                $crate::catalog::translations::write(self, locale, text)
            }
        }

        lookup!($name);
    };
}

/// Adds `text(&locale)`: the message in that locale, or its English fallback.
/// Templates pass `page.locale`, which Askama borrows.
macro_rules! lookup {
    ($name:ident) => {
        impl $name {
            /// This message in `locale`, or its English fallback.
            #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
            pub fn text(self, locale: &$crate::Locale) -> $crate::catalog::fallback::Localized {
                $crate::catalog::Catalog::new(*locale).localized(self)
            }
        }
    };
    ($name:ident<'_>) => {
        impl $name<'_> {
            /// This message in `locale`, or its English fallback.
            #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
            pub fn text(self, locale: &$crate::Locale) -> $crate::catalog::fallback::Localized {
                $crate::catalog::Catalog::new(*locale).localized(self)
            }
        }
    };
}

pub(super) mod browse;
pub(super) mod contributions;
pub(super) mod create;
pub(super) mod edit;
pub(super) mod history;
pub(super) mod nav;
pub(super) mod releases;
pub(super) mod settings;
pub use browse::{RepoBrowseLabel, RepoBrowseMessage, RepoBrowseSentence};
pub use contributions::{ContributionLabel, ContributionMessage};
pub use create::{RepoCreateLabel, RepoCreateMessage};
pub use edit::{RepoEditLabel, RepoEditMessage};
pub use history::{RepoHistoryLabel, RepoHistoryMessage};
pub use nav::{RepoNavLabel, RepoNavMessage, RepoNavSentence};
pub use releases::{ReleaseLabel, ReleaseMessage};
pub use settings::{RepoSettingsLabel, RepoSettingsMessage};
