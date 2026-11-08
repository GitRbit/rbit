//! German (`de`) entries for `catalog/settings/config.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

entries! { De:
    ConfigText<'a> => MessageWriter |text| {}
    ConfigMessage => MessageWriter |text| {}
    ConfigPageText<'a> => MessageWriter |text| {}
}
