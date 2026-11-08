//! Russian (`ru`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { Ru:
    AdminMessage => MessageWriter |text| {}
    DashboardMessage => MessageWriter |text| {}
    MonitoringMessage => MessageWriter |text| {}
    AdminText<'a> => MessageWriter |text| {}
}
