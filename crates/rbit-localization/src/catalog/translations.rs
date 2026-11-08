//! Per-locale translation tables for the families that need only English.
//!
//! Every [`MessageFamily`](super::fallback::MessageFamily) and
//! [`RichFamily`](super::collab::rich::RichFamily) family looks itself up here from its
//! `translate` method. Each translated locale has its own directory, mirroring the source
//! modules: the entries for `catalog/collab/review.rs` in German live in
//! `translations/de/collab/review.rs`. A table lists every family of its source file with
//! the arms translated so far; a message without an arm returns `false` and falls back to
//! English, marked `lang="en"`. Locales never share a file, so translators of different
//! languages never edit the same file. See `docs/design/localization-contracts.md`.
use crate::locale::Locale;

/// One locale's entries for the family `M`, written through `W` (a `MessageWriter` or a
/// `RichWriter`). Implemented by [`entries!`] only.
pub(crate) trait Entries<M, W> {
    /// Writes `message` and returns `true`, or returns `false` when it has no entry.
    fn write(message: M, text: &mut W) -> bool;
}

/// Declares a locale's entries for the families of one source file:
///
/// ```ignore
/// entries! { De:
///     ReviewMessage<'a> => MessageWriter |text| {
///         ReviewMessage::Approve => text.literal("Genehmigen"),
///     }
/// }
/// ```
///
/// Each arm is a match arm on the family and ends with a comma (or is a block). Messages
/// without an arm fall back to English.
macro_rules! entries {
    ($locale:ident: $($family:ident $(<$lifetime:lifetime>)? => $writer:ident |$text:ident| { $($arms:tt)* })*) => {$(
        impl $(<$lifetime>)? $crate::catalog::translations::Entries<$family $(<$lifetime>)?, $writer>
            for $crate::catalog::translations::$locale
        {
            #[allow(
                unreachable_code,
                unreachable_patterns,
                unused_variables,
                clippy::match_single_binding,
                clippy::match_wildcard_for_single_variants,
                clippy::too_many_lines,
                reason = "a table lists only the messages translated so far"
            )]
            fn write(message: $family $(<$lifetime>)?, $text: &mut $writer) -> bool {
                match message {
                    $($arms)*
                    _ => return false,
                }
                true
            }
        }
    )*};
}

/// Spanish (`es`) entries, for the families its original catalog does not cover.
#[derive(Debug)]
pub(crate) struct Es;
/// Simplified Chinese (`zh-Hans`) entries.
#[derive(Debug)]
pub(crate) struct ZhHans;
/// Brazilian Portuguese (`pt-BR`) entries.
#[derive(Debug)]
pub(crate) struct PtBr;
/// Japanese (`ja`) entries.
#[derive(Debug)]
pub(crate) struct Ja;
/// German (`de`) entries.
#[derive(Debug)]
pub(crate) struct De;
/// Russian (`ru`) entries.
#[derive(Debug)]
pub(crate) struct Ru;
/// French (`fr`) entries.
#[derive(Debug)]
pub(crate) struct Fr;
/// Korean (`ko`) entries.
#[derive(Debug)]
pub(crate) struct Ko;

mod de;
mod es;
mod fr;
mod ja;
mod ko;
mod pt_br;
mod ru;
mod zh_hans;

/// Writes `message` from `locale`'s table and returns `true`, or returns `false` when the
/// table has no entry for it (or `locale` is English or a diagnostic).
pub(crate) fn write<M, W>(message: M, locale: Locale, text: &mut W) -> bool
where
    Es: Entries<M, W>,
    ZhHans: Entries<M, W>,
    PtBr: Entries<M, W>,
    Ja: Entries<M, W>,
    De: Entries<M, W>,
    Ru: Entries<M, W>,
    Fr: Entries<M, W>,
    Ko: Entries<M, W>,
{
    match locale {
        Locale::Spanish => Es::write(message, text),
        Locale::ChineseSimplified => ZhHans::write(message, text),
        Locale::PortugueseBrazil => PtBr::write(message, text),
        Locale::Japanese => Ja::write(message, text),
        Locale::German => De::write(message, text),
        Locale::Russian => Ru::write(message, text),
        Locale::French => Fr::write(message, text),
        Locale::Korean => Ko::write(message, text),
        Locale::English | Locale::PseudoExpanded | Locale::PseudoRtl => false,
    }
}

#[cfg(test)]
mod tests;
