//! Spanish (`es`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { Es:
    TokensMessage => MessageWriter |text| {}
    TokenText<'a> => MessageWriter |text| {}
}
