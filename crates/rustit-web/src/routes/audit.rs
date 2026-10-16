//! Administrator audit log: `/admin/audit`.
//!
//! Milestone 1 workstream S owns this file. Behind [`SiteAdmin`], so everyone else gets
//! the ordinary 404 page.

use axum::{
    Router,
    extract::{Query, State},
    routing::get,
};
use rustit_services::{admin::ListQuery, audit::AuditCategory};
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::SiteAdmin,
    views::{admin::AdminSection, layout::Page, security::AuditPage},
};

pub fn routes() -> Router<AppState> {
    Router::new().route(AdminSection::AuditLog.url(), get(audit_log))
}

/// `?q=…&category=…&page=…`, all lenient: anything unparsable means "no filter" or "first page".
#[derive(Debug, Default, Deserialize)]
pub struct AuditParams {
    q: Option<String>,
    category: Option<String>,
    page: Option<String>,
}

/// `GET /admin/audit`
pub async fn audit_log(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Query(params): Query<AuditParams>,
) -> WebResult<AuditPage> {
    let query = ListQuery { search: params.q.unwrap_or_default(), page: params.page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(1) };
    let category = params.category.as_deref().and_then(AuditCategory::parse);
    let paged = state.services.audit.list(&admin, category, &query).await?;
    Ok(AuditPage::new(page, query.search(), category, &paged))
}
