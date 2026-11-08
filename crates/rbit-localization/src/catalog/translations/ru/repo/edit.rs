//! Russian (`ru`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { Ru:
    RepoEditLabel => MessageWriter |text| {}
    RepoEditMessage<'a> => MessageWriter |text| {}
}
