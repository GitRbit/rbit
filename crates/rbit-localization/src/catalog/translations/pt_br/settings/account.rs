//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { PtBr:
    AccountMessage => MessageWriter |text| {}
    AccountText<'a> => MessageWriter |text| {}
}
