//! Russian (`ru`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ru:
    ReviewMessage<'a> => MessageWriter |text| {}
}
