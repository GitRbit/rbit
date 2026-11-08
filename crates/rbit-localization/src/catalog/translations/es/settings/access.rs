//! Spanish (`es`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { Es:
    AccessMessage => MessageWriter |text| {}
    AccessText<'a> => MessageWriter |text| {}
}
