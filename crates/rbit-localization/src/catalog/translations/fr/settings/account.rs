//! French (`fr`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { Fr:
    AccountMessage => MessageWriter |text| {}
    AccountText<'a> => MessageWriter |text| {}
}
