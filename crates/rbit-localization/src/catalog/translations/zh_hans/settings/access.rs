//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/access.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { ZhHans:
    AccessMessage => MessageWriter |text| {}
    AccessText<'a> => MessageWriter |text| {}
}
