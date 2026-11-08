//! Russian (`ru`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};

entries! { Ru:
    ReleaseLabel => MessageWriter |text| {}
    ReleaseMessage<'a> => MessageWriter |text| {}
}
