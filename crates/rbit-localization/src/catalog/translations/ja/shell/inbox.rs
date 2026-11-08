//! Japanese (`ja`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { Ja:
    InboxMessage<'a> => MessageWriter |text| {}
}
