//! Korean (`ko`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { Ko:
    RepoEditLabel => MessageWriter |text| {}
    RepoEditMessage<'a> => MessageWriter |text| {}
}
