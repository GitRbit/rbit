//! Spanish (`es`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};

entries! { Es:
    AdminPageMessage => MessageWriter |text| {}
    InstanceSettingsMessage => MessageWriter |text| {}
    AdminPageText<'a> => MessageWriter |text| {}
}
