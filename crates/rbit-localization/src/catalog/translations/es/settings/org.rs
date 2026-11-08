//! Spanish (`es`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};

entries! { Es:
    OrgMessage => MessageWriter |text| {}
    OrgText<'a> => MessageWriter |text| {}
}
