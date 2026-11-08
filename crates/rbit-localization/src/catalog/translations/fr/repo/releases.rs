//! French (`fr`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};

entries! { Fr:
    ReleaseLabel => MessageWriter |text| {}
    ReleaseMessage<'a> => MessageWriter |text| {}
}
