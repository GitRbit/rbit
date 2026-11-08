//! French (`fr`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Fr:
    SecurityMessage => MessageWriter |text| {}
    SecurityText<'a> => MessageWriter |text| {}
}
