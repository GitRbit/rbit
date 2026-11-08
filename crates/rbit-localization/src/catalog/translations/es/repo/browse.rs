//! Spanish (`es`) entries for `catalog/repo/browse.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::browse::{RepoBrowseLabel, RepoBrowseMessage};

entries! { Es:
    RepoBrowseLabel => MessageWriter |text| {}
    RepoBrowseMessage<'a> => MessageWriter |text| {}
}
