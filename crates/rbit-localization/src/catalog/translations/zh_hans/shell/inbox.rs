//! Simplified Chinese (`zh-Hans`) entries for `catalog/shell/inbox.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::inbox::InboxMessage;

entries! { ZhHans:
    InboxMessage<'a> => MessageWriter |text| {}
}
