//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};

entries! { ZhHans:
    TokensMessage => MessageWriter |text| {}
    TokenText<'a> => MessageWriter |text| {}
}
