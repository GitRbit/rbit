//! French (`fr`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { Fr:
    InboxMessage<'a> => MessageWriter |text| {}
}
