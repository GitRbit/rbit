//! Selected readable source projections retain existing fields and order.
use crate::{
    IssueState,
    threads::{IssueId, IssueKind, IssueNumber},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRef {
    pub id: IssueId,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub state: IssueState,
    pub title: String,
}

/// A thread that mentions another one (`CrossReferenced`), with the
/// repository it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueSource {
    pub issue: IssueRef,
    /// `owner/name` of the source's repository.
    pub full_name: String,
}
