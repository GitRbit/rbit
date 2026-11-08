//! Russian (`ru`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};

entries! { Ru:
    ContributionLabel => MessageWriter |text| {}
    ContributionMessage<'a> => MessageWriter |text| {}
}
