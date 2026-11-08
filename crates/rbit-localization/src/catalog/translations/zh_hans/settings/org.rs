//! Simplified Chinese (`zh-Hans`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { ZhHans:
    OrgMessage => MessageWriter |text| {}
    OrgText<'a> => MessageWriter |text| {}
}
