//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { PtBr:
    SecurityMessage => MessageWriter |text| {}
    SecurityText<'a> => MessageWriter |text| {}
}
