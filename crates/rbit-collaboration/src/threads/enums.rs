//! Original stable text-enum contracts for numbered threads.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Generates `as_str`, `parse` and `ALL` for a field-less enum stored as text.
macro_rules! text_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stable identifier stored in the database and used in payloads.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

text_enum!(
    /// Whether a numbered thread is a plain issue or a pull request.
    IssueKind {
        Issue => "issue",
        Pull => "pull",
    }
);

text_enum!(
    /// Why a thread was closed. `Merged` is only possible for pull requests.
    StateReason {
        Completed => "completed",
        NotPlanned => "not_planned",
        Merged => "merged",
    }
);
