//! Russian (`ru`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { Ru:
    TokensMessage => MessageWriter |text| {}
    TokenText<'a> => MessageWriter |text| {}
}
