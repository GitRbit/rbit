//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/history.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::history::{RepoHistoryLabel, RepoHistoryMessage};

entries! { PtBr:
    RepoHistoryLabel => MessageWriter |text| {}
    RepoHistoryMessage<'a> => MessageWriter |text| {}
}
