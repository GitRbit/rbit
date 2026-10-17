//! REST API: Pull requests, reviews, review comments, requested reviewers and merging:
//! `/repos/{owner}/{repo}/pulls…`.
//!
//! Milestone 2 workstream P (pull requests) owns this file and `openapi/pulls.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.

use super::ApiRoute;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    Vec::new()
}
