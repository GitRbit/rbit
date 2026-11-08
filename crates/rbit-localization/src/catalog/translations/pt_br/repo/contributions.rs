//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};

entries! { PtBr:
    ContributionLabel => MessageWriter |text| {}
    ContributionMessage<'a> => MessageWriter |text| {}
}
