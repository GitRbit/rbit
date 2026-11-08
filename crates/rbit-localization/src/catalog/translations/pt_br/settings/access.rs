//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { PtBr:
    AccessMessage => MessageWriter |text| {}
    AccessText<'a> => MessageWriter |text| {}
}
