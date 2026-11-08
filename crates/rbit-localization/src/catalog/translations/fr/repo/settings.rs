//! French (`fr`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};

entries! { Fr:
    RepoSettingsLabel => MessageWriter |text| {}
    RepoSettingsMessage<'a> => MessageWriter |text| {}
}
