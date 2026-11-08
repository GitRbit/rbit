//! Russian (`ru`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};

entries! { Ru:
    ProtectionMessage => MessageWriter |text| {}
    ProtectionText<'a> => MessageWriter |text| {}
    RuleBadge => MessageWriter |text| {}
}
