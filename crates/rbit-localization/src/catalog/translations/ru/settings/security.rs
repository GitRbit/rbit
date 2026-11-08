//! Russian (`ru`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Ru:
    SecurityMessage => MessageWriter |text| {}
    SecurityText<'a> => MessageWriter |text| {}
}
