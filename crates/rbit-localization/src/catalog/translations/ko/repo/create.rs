//! Korean (`ko`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};

entries! { Ko:
    RepoCreateLabel => MessageWriter |text| {}
    RepoCreateMessage<'a> => MessageWriter |text| {}
}
