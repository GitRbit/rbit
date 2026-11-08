//! Japanese (`ja`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Ja:
    ReviewMessage<'a> => MessageWriter |text| {}
}
