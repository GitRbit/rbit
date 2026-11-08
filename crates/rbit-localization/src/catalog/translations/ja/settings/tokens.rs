//! Japanese (`ja`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { Ja:
    TokensMessage => MessageWriter |text| {}
    TokenText<'a> => MessageWriter |text| {}
}
