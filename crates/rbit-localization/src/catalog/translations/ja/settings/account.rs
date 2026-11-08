//! Japanese (`ja`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { Ja:
    AccountMessage => MessageWriter |text| {}
    AccountText<'a> => MessageWriter |text| {}
}
