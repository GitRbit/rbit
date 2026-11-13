//! The Unicode 17 NFC contract of persisted wiki names, including mark admission.
//!
//! The official published 0.1.25 source is pinned separately from current registry
//! consumers. Updating ordinary dependencies must not change catalog keys, retained
//! titles, aliases or exact replay payloads. See docs/dependency-maintenance-0.2.2.md.

use unicode_normalization_v17::UnicodeNormalization;

const _: () = assert!(matches!(unicode_normalization_v17::UNICODE_VERSION, (17, 0, 0)));

pub(super) fn is_canonical(value: &str) -> bool {
    unicode_normalization_v17::is_nfc(value)
}

pub(super) fn canonicalize(value: &str) -> String {
    value.nfc().collect()
}

pub(super) fn is_combining_mark(character: char) -> bool {
    unicode_normalization_v17::char::is_combining_mark(character)
}
