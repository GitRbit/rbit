//! Korean (`ko`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};

entries! { Ko:
    TeamMessage => MessageWriter |text| {}
    TeamText<'a> => MessageWriter |text| {}
}
