//! Branch protection settings: `/{owner}/{repo}/settings/branches`, `/settings/branches/new`,
//! `/settings/branches/{id}`.
//!
//! Milestone 2 workstream B (branch protection) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Only repository administrators get these pages;
//! everyone else gets 404, as do rule ids the repository does not have.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::protection::{ProtectionId, RuleSettings};
use rbit_git::RefKind;
use rbit_services::{ServiceError, protection::ProtectionInput};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        protection::{RuleForm, RuleFormPage, RulesPage, rules_url},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/settings/branches", get(list).post(create))
        .route("/{owner}/{repo}/settings/branches/new", get(new_form))
        .route("/{owner}/{repo}/settings/branches/{id}", get(show).post(update))
        .route("/{owner}/{repo}/settings/branches/{id}/delete", post(delete))
}

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
async fn load_for_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

/// A rule id from the path; anything else is 404.
fn rule_id(id: &str) -> WebResult<ProtectionId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(ProtectionId).ok_or(WebError::NotFound)
}

/// The repository's branch names, from the refs the page loaded anyway.
fn branch_names(loaded: &RepoPage) -> Vec<String> {
    loaded.refs().iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name.clone()).collect()
}

/// The submitted rule form: checkboxes are present only when ticked, `team` once per
/// ticked team, so it is read as a list of pairs.
#[derive(Debug, Default)]
pub struct RuleFormInput {
    pairs: Vec<(String, String)>,
}

impl RuleFormInput {
    fn value(&self, name: &str) -> Option<&str> {
        self.pairs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    fn ticked(&self, name: &str) -> bool {
        self.value(name).is_some()
    }

    fn input(&self) -> ProtectionInput {
        let settings = RuleSettings {
            deny_force_push: self.ticked("deny_force_push"),
            deny_deletion: self.ticked("deny_deletion"),
            require_pull_request: self.ticked("require_pull_request"),
            // Out of range values fail validation with a clear message.
            required_approvals: self.value("required_approvals").map_or(Some(0), |value| value.trim().parse().ok()).unwrap_or(u8::MAX),
            dismiss_stale_approvals: self.ticked("dismiss_stale_approvals"),
            block_on_changes_requested: self.ticked("block_on_changes_requested"),
            restrict_pushers: self.ticked("restrict_pushers"),
            allow_admin_bypass: self.ticked("allow_admin_bypass"),
        };
        let users = self.value("users").unwrap_or_default();
        ProtectionInput {
            pattern: self.value("pattern").unwrap_or_default().to_owned(),
            settings,
            users: users.split(|c: char| c == ',' || c.is_whitespace()).filter(|name| !name.is_empty()).map(str::to_owned).collect(),
            teams: self.pairs.iter().filter(|(key, _)| key == "team").map(|(_, value)| value.clone()).collect(),
        }
    }
}

impl<'de> Deserialize<'de> for RuleFormInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self { pairs: Vec::deserialize(deserializer)? })
    }
}

#[derive(Debug, Deserialize)]
pub struct NoticeQuery {
    saved: Option<String>,
    deleted: Option<String>,
}

/// `GET /{owner}/{repo}/settings/branches`
pub async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<NoticeQuery>,
) -> WebResult<RulesPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let rules = state.services.protection.list(&loaded.ctx).await?;
    Ok(RulesPage::new(page, loaded.header(&state, "settings"), &rules, &branch_names(&loaded)).deleted(query.deleted.is_some()))
}

/// The empty form for a new rule.
async fn new_page(state: &AppState, loaded: &RepoPage, page: Page) -> WebResult<RuleFormPage> {
    let teams = state.services.protection.teams(&loaded.ctx).await?;
    Ok(RuleFormPage::new_rule(page, loaded.header(state, "settings"), loaded.ctx.repo.owner.is_organization(), &teams))
}

/// `GET /{owner}/{repo}/settings/branches/new`
pub async fn new_form(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<RuleFormPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    new_page(&state, &loaded, page).await
}

/// `POST /{owner}/{repo}/settings/branches`: adds the rule and shows it.
pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<RuleFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let input = form.input();
    match state.services.protection.create(&user, &loaded.ctx, &input).await {
        Ok(rule) => {
            let header = loaded.header(&state, "settings");
            Ok(Redirect::to(&format!("{}/{}?saved=1", rules_url(&header), rule.id.0)).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            let teams = state.services.protection.teams(&loaded.ctx).await?;
            let rendered =
                new_page(&state, &loaded, page).await?.rejected(RuleForm::new(&input, &teams), FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// The edit page of rule `id`.
async fn edit_page(state: &AppState, loaded: &RepoPage, page: Page, id: ProtectionId) -> WebResult<RuleFormPage> {
    let protection = &state.services.protection;
    let rule = protection.get(&loaded.ctx, id).await?;
    let teams = protection.teams(&loaded.ctx).await?;
    let is_org = loaded.ctx.repo.owner.is_organization();
    Ok(RuleFormPage::edit(page, loaded.header(state, "settings"), is_org, &teams, &rule, &branch_names(loaded)))
}

/// `GET /{owner}/{repo}/settings/branches/{id}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Query(query): Query<NoticeQuery>,
) -> WebResult<RuleFormPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    Ok(edit_page(&state, &loaded, page, rule_id(&id)?).await?.saved(query.saved.is_some()))
}

/// `POST /{owner}/{repo}/settings/branches/{id}`
pub async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<RuleFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let id = rule_id(&id)?;
    let input = form.input();
    match state.services.protection.update(&user, &loaded.ctx, id, &input).await {
        Ok(rule) => {
            let header = loaded.header(&state, "settings");
            Ok(Redirect::to(&format!("{}/{}?saved=1", rules_url(&header), rule.id.0)).into_response())
        }
        Err(ServiceError::Invalid(message)) => {
            let teams = state.services.protection.teams(&loaded.ctx).await?;
            let rendered =
                edit_page(&state, &loaded, page, id).await?.rejected(RuleForm::new(&input, &teams), FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteFormInput {
    confirm: Option<String>,
}

/// `POST /{owner}/{repo}/settings/branches/{id}/delete`: needs the confirmation box ticked.
pub async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<DeleteFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let id = rule_id(&id)?;
    if form.confirm.is_none() {
        let rendered = edit_page(&state, &loaded, page, id).await?.delete_rejected(FormError::new("Tick the box to confirm."));
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response());
    }
    state.services.protection.delete(&user, &loaded.ctx, id).await?;
    let header = loaded.header(&state, "settings");
    Ok(Redirect::to(&format!("{}?deleted=1", rules_url(&header))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(pairs: &[(&str, &str)]) -> RuleFormInput {
        RuleFormInput { pairs: pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect() }
    }

    #[test]
    fn the_form_reads_checkboxes_teams_and_usernames() {
        let input = form(&[
            ("pattern", "release/*"),
            ("deny_force_push", "on"),
            ("require_pull_request", "on"),
            ("required_approvals", "2"),
            ("restrict_pushers", "on"),
            ("users", "alice, bob\ncarol"),
            ("team", "core"),
            ("team", "docs"),
        ])
        .input();
        assert_eq!(input.pattern, "release/*");
        let settings = input.settings;
        assert!(settings.deny_force_push && !settings.deny_deletion && settings.require_pull_request && settings.restrict_pushers);
        assert!(!settings.block_on_changes_requested && !settings.allow_admin_bypass);
        assert_eq!(settings.required_approvals, 2);
        assert_eq!(input.users, ["alice", "bob", "carol"]);
        assert_eq!(input.teams, ["core", "docs"]);

        assert_eq!(form(&[("required_approvals", "lots")]).input().settings.required_approvals, u8::MAX);
        assert_eq!(form(&[]).input().settings.required_approvals, 0);
        assert!(rule_id("0").is_err() && rule_id("x").is_err() && rule_id("-1").is_err());
        assert_eq!(rule_id("12").ok(), Some(ProtectionId(12)));
    }
}
