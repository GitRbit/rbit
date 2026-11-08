//! Simplified Chinese (`zh-Hans`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};

entries! { ZhHans:
    RepoEditLabel => MessageWriter |text| {}
    RepoEditMessage<'a> => MessageWriter |text| {}
}
