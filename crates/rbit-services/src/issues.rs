//! Issues: creation with metadata, labels, milestones, assignees, commit references and
//! search. The comments, state changes and timeline shared with pull requests live in
//! [`crate::timeline`].
//!
//! Milestone 2 workstream I (issues) owns this file and `issues/**`.
//!
//! Hooks other workstreams call (signatures fixed by `docs/design/milestone-2b.md` §3):
//! [`IssueService::metadata`], [`hooks::payload`] and [`json`].

pub mod hooks;
pub mod json;

use std::collections::HashMap;

use rbit_core::{Account, IssueId, MilestoneId, issue::LabelSnapshot};

use crate::{ServiceContext, ServiceError, ThrottleService, TimelineService, references::ReferenceService};

/// Labels, assignees and milestone of one issue or pull request, for lists, payloads and
/// the API.
#[derive(Debug, Clone, Default)]
pub struct IssueMetadata {
    pub labels: Vec<LabelSnapshot>,
    pub assignees: Vec<Account>,
    pub milestone: Option<MilestoneRef>,
}

/// The milestone an issue belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneRef {
    pub id: MilestoneId,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct IssueService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    timeline: TimelineService,
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    references: ReferenceService,
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    throttle: ThrottleService,
}

impl IssueService {
    /// `timeline` and `throttle` are the instances every service shares, so creating an
    /// issue spends the same content budget as commenting.
    pub(crate) fn new(ctx: ServiceContext, timeline: TimelineService, references: ReferenceService, throttle: ThrottleService) -> Self {
        Self { ctx, timeline, references, throttle }
    }

    /// Labels, assignees and milestone of each of `issues` (missing ids are left out).
    /// One batch query per kind of metadata. Stub: an empty map.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn metadata(&self, _issues: &[IssueId]) -> Result<HashMap<IssueId, IssueMetadata>, ServiceError> {
        Ok(HashMap::new())
    }

    /// Starts this service's background work (commit references on `RepositoryPushed`).
    /// Called once from [`crate::Services::spawn_background`]. Stub until the workstream lands.
    pub fn spawn_background(&self) {}

    /// Stops background work and waits (bounded) for it to finish. Called from
    /// [`crate::Services::shutdown`] before the stores close.
    #[expect(clippy::unused_async, reason = "stub until the workstream implements background work")]
    pub async fn shutdown(&self) {}
}
