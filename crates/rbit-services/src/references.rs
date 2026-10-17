//! Resolving references (`#12`, `owner/repo#12`, `@name`) found in Markdown, so pages
//! can link the ones that exist, and recording cross-references between threads.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! [`ReferenceService::resolve`] is a hook (`docs/design/milestone-2b.md` §3): T0's
//! timeline views and P call it before rendering with
//! `rbit_web::render::references::render_issue_markdown`.

use crate::{RepoContext, ServiceContext, ServiceError, TimelineService};

/// What the references in a batch of texts resolve to: the existing issues of the
/// repository among `#N` and the existing accounts among `@name`. I defines its fields;
/// the default resolves nothing (every reference renders as plain text).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {}

#[derive(Debug, Clone)]
pub struct ReferenceService {
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    ctx: ServiceContext,
    #[expect(dead_code, reason = "stub until the workstream implements the service")]
    timeline: TimelineService,
}

impl ReferenceService {
    pub(crate) fn new(ctx: ServiceContext, timeline: TimelineService) -> Self {
        Self { ctx, timeline }
    }

    /// Resolves the `#N` and `@name` references among `texts` in `repo` (already
    /// access-checked), with one batch query for each kind. Stub: nothing resolves.
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "stub until the workstream implements the service")]
    pub async fn resolve(&self, _repo: &RepoContext, _texts: &[&str]) -> Result<ReferenceSet, ServiceError> {
        Ok(ReferenceSet::default())
    }
}
