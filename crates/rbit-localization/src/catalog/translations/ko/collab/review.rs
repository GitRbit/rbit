//! Korean (`ko`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ko:
    ReviewMessage<'a> => MessageWriter |text| {}
}
