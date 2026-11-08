//! Russian (`ru`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { Ru:
    AccountMessage => MessageWriter |text| {}
    AccountText<'a> => MessageWriter |text| {}
}
