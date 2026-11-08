//! Korean (`ko`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { Ko:
    InboxMessage<'a> => MessageWriter |text| {}
}
