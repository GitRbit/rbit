//! Korean (`ko`) entries for `catalog/settings/keys.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::keys::{KeyLabel, KeysMessage};

entries! { Ko:
    KeysMessage => MessageWriter |text| {}
    KeyLabel<'a> => MessageWriter |text| {}
}
