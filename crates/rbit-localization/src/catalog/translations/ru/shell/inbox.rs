//! Russian (`ru`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { Ru:
    InboxMessage<'a> => MessageWriter |text| {}
}
