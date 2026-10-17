//! REST API: Branch protection rules: `/repos/{owner}/{repo}/branch_protections…` (admin scope).
//!
//! Milestone 2 workstream B (branch protection) owns this file and `openapi/protection.json`, the `OpenAPI`
//! fragment documenting exactly these routes (merged into the served document).
//!
//! A child of `api`, so it can use the parent's private `route()` helper, `Caller`,
//! `Pager`, `ApiError` and `ApiJson`.

use super::ApiRoute;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    Vec::new()
}
