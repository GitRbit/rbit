//! Japanese (`ja`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};

entries! { Ja:
    RepoSettingsLabel => MessageWriter |text| {}
    RepoSettingsMessage<'a> => MessageWriter |text| {}
}
