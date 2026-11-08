//! French (`fr`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    ReviewMessage<'a> => MessageWriter |text| {}
}
