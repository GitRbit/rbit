//! German (`de`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { De:
    InboxMessage<'a> => MessageWriter |text| {}
}
