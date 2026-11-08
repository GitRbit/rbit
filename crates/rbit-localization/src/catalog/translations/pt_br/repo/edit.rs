//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { PtBr:
    RepoEditLabel => MessageWriter |text| {}
    RepoEditMessage<'a> => MessageWriter |text| {}
}
