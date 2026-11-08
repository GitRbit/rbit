//! English-only message families for account settings and administration.
//!
//! Each family implements [`MessageFamily`](super::MessageFamily) with English alone,
//! so every other locale falls back to English (rendered `lang="en"`) until a
//! translator adds a `translate` arm. Each family also has `text(&locale)`, which
//! templates call with the page locale.
//!
//! Sentences that wrap a link or a machine value in markup are split at that markup
//! into `…Lead` and `…Tail` fragments; the markup itself stays in the template.

/// Adds `text(&locale)`: the message in `locale`, or its English fallback.
macro_rules! localizes {
    ($name:ident $(<$lifetime:lifetime>)?) => {
        impl $(<$lifetime>)? $name $(<$lifetime>)? {
            /// This message in `locale`, or its English fallback.
            #[allow(clippy::trivially_copy_pass_by_ref, reason = "templates pass the page locale by reference")]
            pub fn text(self, locale: &$crate::Locale) -> $crate::catalog::fallback::Localized {
                $crate::catalog::Catalog::new(*locale).localized(self)
            }
        }
    };
}

/// Declares a family of fixed labels, one `Variant = "key" => "English",` per message.
/// Keys are prefixed with the family's namespace.
macro_rules! labels {
    ($(#[$meta:meta])* $name:ident, $prefix:literal { $($variant:ident = $key:literal => $english:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            $($variant,)+
        }

        impl $name {
            /// Every message of the family, for catalog checks.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];
        }

        localizes!($name);

        impl $crate::catalog::fallback::MessageFamily for $name {
            fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!($prefix, ".", $key),)+
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
    };
}

/// Writes one piece of a message: `"wording"`, `{name}` for a text argument or
/// `(count)` for a number.
macro_rules! part {
    ($text:ident, {$value:ident}) => {
        $text.argument($value)
    };
    ($text:ident, ($value:ident)) => {
        $text.number($value)
    };
    ($text:ident, $literal:literal) => {
        $text.literal($literal)
    };
}

/// Declares a family of messages with arguments:
/// `Variant(name: &'a str) = "key" => ["Delete ", {name}],`.
macro_rules! messages {
    ($(#[$meta:meta])* $name:ident $(<$lifetime:lifetime>)?, $prefix:literal {
        $($variant:ident($($field:ident: $type:ty),+) = $key:literal => [$($part:tt),+],)+
    }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name $(<$lifetime>)? {
            $($variant($($type),+),)+
        }

        localizes!($name $(<$lifetime>)?);

        impl $(<$lifetime>)? $crate::catalog::fallback::MessageFamily for $name $(<$lifetime>)? {
            fn id(self) -> &'static str {
                match self {
                    $(Self::$variant(..) => concat!($prefix, ".", $key),)+
                }
            }

            fn english(self, text: &mut $crate::catalog::fallback::MessageWriter) {
                match self {
                    $(Self::$variant($($field),+) => {
                        $(part!(text, $part);)+
                    })+
                }
            }

            fn translate(self, locale: $crate::locale::Locale, text: &mut $crate::catalog::fallback::MessageWriter) -> bool {
                $crate::catalog::translations::write(self, locale, text)
            }
        }
    };
}

pub mod access;
pub mod account;
pub mod admin;
pub mod admin_pages;
pub mod cards;
pub mod config;
pub mod keys;
pub mod org;
pub mod org_teams;
pub mod profile;
pub mod protection;
pub mod security;
pub mod tokens;
pub mod webhooks;
