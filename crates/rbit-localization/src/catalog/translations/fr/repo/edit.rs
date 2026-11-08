//! French (`fr`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { Fr:
    RepoEditLabel => MessageWriter |text| {}
    RepoEditMessage<'a> => MessageWriter |text| {}
}
