//! REST API: Issues, comments, labels, milestones, assignees and the timeline:
//! `/repos/{owner}/{repo}/issues…`, `/labels…`, `/milestones…`, `/assignees`.
//!
//! Milestone 2 workstream I (issues) owns this file and `openapi/issues.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.

use super::ApiRoute;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    Vec::new()
}
