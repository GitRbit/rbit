//! Repository, organization and personal Actions configuration documents.
use crate::{
    routes::actions::settings as old,
    state::WeakAppState,
    views::{
        actions_settings::{Notice, RepoActionsSettingsPage},
        components::FormError,
    },
};
use rbit_core::{AccountId, actions::DefaultTokenPermission};
use rbit_services::{
    ActionsRouteBundle, ServiceError,
    actions::secrets::ActionsTarget,
    plugin_pages::{
        FormField, FormFieldKind, FormReference, FormSchema, MutationAudience, MutationDescriptor, MutationError, MutationOutcome,
        PageAudience, PageDescriptor, PageScope, PluginRedirect, compiled_mutation, compiled_request_page,
    },
};
use std::time::Duration;

pub(super) fn text(name: &str, max_bytes: usize, multiline: bool, required: bool) -> FormField {
    FormField { name: name.into(), kind: FormFieldKind::Text { max_bytes, multiline }, required }
}
pub(super) fn entry_schema(extra: Vec<FormField>) -> Result<FormSchema, ServiceError> {
    let mut fields = vec![
        text("kind", 16, false, false),
        text("action", 16, false, false),
        text("name", 100, false, true),
        text("value", 65536, true, false),
        text("visibility", 16, false, false),
        FormField {
            name: "repository".into(),
            kind: FormFieldKind::TextList { max_items: 128, max_bytes: 20, multiline: false },
            required: false,
        },
    ];
    fields.extend(extra);
    FormSchema::large_text(fields).map_err(super::declaration)
}
pub(super) fn descriptor(
    id: &str,
    path: &str,
    scope: PageScope,
    audience: MutationAudience,
    schema: FormSchema,
) -> Result<MutationDescriptor, ServiceError> {
    MutationDescriptor::new(id, scope, path, audience, Duration::from_secs(1), schema)
        .map(MutationDescriptor::with_document)
        .and_then(|d| d.with_body_limit(2 * 1024 * 1024))
        .map_err(super::declaration)
}
pub(super) fn redirect(path: &str, notice: Notice, extra: &[(&str, &str)]) -> Result<PluginRedirect, MutationError> {
    let mut query = vec![("done", notice.key())];
    query.extend_from_slice(extra);
    PluginRedirect::new(path)?.with_query(&query)
}
fn service(error: ServiceError) -> MutationError {
    super::mutation_error(error.into())
}

pub(super) fn bundle(weak: &WeakAppState) -> Result<ActionsRouteBundle, ServiceError> {
    let mut bundle = ActionsRouteBundle::default();
    let forms = ["settings", "settings/secrets", "settings/variables", "settings/trusted"]
        .iter()
        .enumerate()
        .map(|(i, path)| FormReference::new(&format!("form-{i}"), path))
        .collect::<Result<_, _>>()
        .map_err(super::declaration)?;
    let d = PageDescriptor::new("settings", PageScope::Repository, "settings", PageAudience::RepositoryAdmin, Duration::from_secs(1))
        .and_then(|d| d.with_query(&["done", "edit", "after", "secret_edit", "secret_after"]))
        .and_then(|d| d.with_forms(forms))
        .map_err(super::declaration)?
        .with_document()
        .with_body_limit(2 * 1024 * 1024)
        .map_err(super::declaration)?;
    let w = (*weak).clone();
    bundle.pages.push(compiled_request_page(d, move |context, forms, query| {
        let weak = w.clone();
        async move {
            let (state, viewer, mut page, owner, repo) = super::request(&weak, &context, forms).await?;
            page.path = super::repo_url(&owner, &repo, "settings");
            page.plugin_query = query.clone();
            let loaded = old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::page_error)?;
            let view = old::repo_page(&state, &loaded, page, query.get("done").and_then(Notice::parse)).await.map_err(super::page_error)?;
            super::document(view, context.locale)
        }
    }));
    for kind in ["secret", "variable"] {
        let path = if kind == "secret" { "settings/secrets" } else { "settings/variables" };
        let d = descriptor(&format!("repo-{kind}"), path, PageScope::Repository, MutationAudience::RepositoryAdmin, entry_schema(vec![])?)?;
        let w = (*weak).clone();
        bundle.mutations.push(compiled_mutation(d, move |context, values| {
            let weak = w.clone();
            async move {
                let (state, user, viewer, mut page, owner, repo) = super::mutation(&weak, &context).await?;
                page.path = super::repo_url(&owner, &repo, "settings");
                let loaded = old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
                let mut form = old::EntryForm::parse(values.to_pairs());
                form.kind = kind.into();
                let actions = old::actions(&state).map_err(super::mutation_error)?;
                let target = ActionsTarget::Repository(&loaded.ctx);
                let result = if kind == "secret" {
                    old::apply_secret(&actions, &user, target, &form).await
                } else {
                    old::apply_variable(&actions, &user, target, &form).await
                };
                match result {
                    Ok(notice) => Ok(MutationOutcome::Redirect(redirect("settings", notice, &[])?)),
                    Err(ServiceError::Invalid(message)) => {
                        let mut view = old::repo_page(&state, &loaded, page, None).await.map_err(super::mutation_error)?;
                        (view.secrets, view.variables) = old::reject(view.secrets, view.variables, &form, &message);
                        Ok(MutationOutcome::Invalid(super::document(view, context.locale)?))
                    }
                    Err(error) => Err(service(error)),
                }
            }
        }));
    }
    repo_controls(&mut bundle, weak)?;
    owner_routes(&mut bundle, weak)?;
    Ok(bundle)
}

fn repo_controls(bundle: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    let d = descriptor(
        "repo-settings",
        "settings",
        PageScope::Repository,
        MutationAudience::RepositoryAdmin,
        FormSchema::new(vec![text("enabled", 8, false, false), text("token_permission", 32, false, true)]).map_err(super::declaration)?,
    )?;
    let w = (*weak).clone();
    bundle.mutations.push(compiled_mutation(d, move |context, values| {
        let weak = w.clone();
        async move {
            let (state, user, viewer, page, owner, repo) = super::mutation(&weak, &context).await?;
            let loaded = old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            let Some(permission) = values.text("token_permission").and_then(DefaultTokenPermission::parse) else {
                let mut view = old::repo_page(&state, &loaded, page, None).await.map_err(super::mutation_error)?;
                view.general_error = FormError::new("Choose read or read and write.");
                return Ok(MutationOutcome::Invalid(super::document(view, context.locale)?));
            };
            let actions = old::actions(&state).map_err(super::mutation_error)?;
            actions.set_repo_enabled(&user, &loaded.ctx, values.text("enabled").is_some()).await.map_err(service)?;
            actions.set_token_permission(&user, &loaded.ctx, permission).await.map_err(service)?;
            Ok(MutationOutcome::Redirect(redirect("settings", Notice::Saved, &[])?))
        }
    }));
    let d = descriptor(
        "trusted",
        "settings/trusted",
        PageScope::Repository,
        MutationAudience::RepositoryAdmin,
        FormSchema::new(vec![FormField { name: "user".into(), kind: FormFieldKind::Integer { min: 1, max: i64::MAX }, required: true }])
            .map_err(super::declaration)?,
    )?;
    let w = (*weak).clone();
    bundle.mutations.push(compiled_mutation(d, move |context, values| {
        let weak = w.clone();
        async move {
            let (state, user, viewer, _, owner, repo) = super::mutation(&weak, &context).await?;
            let loaded = old::load_admin(&state, &viewer, &owner, &repo).await.map_err(super::mutation_error)?;
            old::actions(&state)
                .map_err(super::mutation_error)?
                .untrust_user(&user, &loaded.ctx, AccountId(values.integer("user").ok_or(MutationError::InvalidForm)?))
                .await
                .map_err(service)?;
            Ok(MutationOutcome::<super::Localized<RepoActionsSettingsPage>>::Redirect(redirect("settings", Notice::Untrusted, &[])?))
        }
    }));
    Ok(())
}

fn owner_routes(bundle: &mut ActionsRouteBundle, weak: &WeakAppState) -> Result<(), ServiceError> {
    macro_rules! register_owner {
        ($path:literal,$id:literal,$organization:literal,$view:ty,$build:expr) => {{
            let d = PageDescriptor::new($id, PageScope::Global, $path, PageAudience::SignedIn, Duration::from_secs(1))
                .and_then(|d| d.with_query(&["owner", "done", "edit", "after", "secret_edit", "secret_after"]))
                .and_then(|d| d.with_forms(vec![FormReference::new("save", $path)?]))
                .map_err(super::declaration)?
                .with_document()
                .with_body_limit(2 * 1024 * 1024)
                .map_err(super::declaration)?;
            let w = (*weak).clone();
            bundle.pages.push(compiled_request_page(d, move |context, forms, query| {
                let weak = w.clone();
                async move {
                    let state = weak.upgrade().ok_or(rbit_services::plugin_pages::PageError::Unavailable)?;
                    let viewer = super::principal(&state, &context).await?;
                    let user = viewer.user().ok_or(rbit_services::plugin_pages::PageError::Forbidden)?;
                    let owner = query.get("owner").unwrap_or_default();
                    let mut page = super::page(&state, &viewer, context.locale.clone(), forms, &super::global_url($path));
                    page.plugin_query = query.clone();
                    if $organization {
                        page.plugin_fields.push(("owner".into(), owner.into()));
                    }
                    let view: $view =
                        ($build)(&state, user, owner, page, query.get("done").and_then(Notice::parse)).await.map_err(super::page_error)?;
                    super::document(view, context.locale)
                }
            }));
            let extra = if $organization { vec![text("owner", 39, false, true)] } else { vec![] };
            let d = descriptor($id, $path, PageScope::Global, MutationAudience::SignedIn, entry_schema(extra)?)?;
            let w = (*weak).clone();
            bundle.mutations.push(compiled_mutation(d, move |context, values| {
                let weak = w.clone();
                async move {
                    let state = weak.upgrade().ok_or(MutationError::Unavailable)?;
                    let user = super::actor(&state, context.actor.account_id).await.map_err(|_| MutationError::Forbidden)?;
                    let viewer = crate::extract::Viewer(Some(std::sync::Arc::new(user.clone())));
                    let owner = values.text("owner").unwrap_or_default();
                    let mut page = super::page(&state, &viewer, context.locale.clone(), context.forms().clone(), &super::global_url($path));
                    if $organization {
                        page.plugin_fields.push(("owner".into(), owner.into()));
                    }
                    let form = old::EntryForm::parse(values.to_pairs());
                    let actions = old::actions(&state).map_err(super::mutation_error)?;
                    let org = if $organization {
                        Some(old::managed_org(&state, &user, owner).await.map_err(super::mutation_error)?)
                    } else {
                        None
                    };
                    let target = org.as_ref().map_or(ActionsTarget::User(&user), ActionsTarget::Organization);
                    match old::apply(&actions, &user, target, &form).await {
                        Ok(notice) => {
                            let owner_query = [("owner", owner)];
                            let extra = if $organization { owner_query.as_slice() } else { &[] };
                            Ok(MutationOutcome::Redirect(redirect($path, notice, extra)?))
                        }
                        Err(ServiceError::Invalid(message)) => {
                            let mut view: $view = ($build)(&state, &user, owner, page, None).await.map_err(super::mutation_error)?;
                            (view.secrets, view.variables) = old::reject(view.secrets, view.variables, &form, &message);
                            Ok(MutationOutcome::Invalid(super::document(view, context.locale)?))
                        }
                        Err(error) => Err(service(error)),
                    }
                }
            }));
        }};
    }
    register_owner!("settings", "personal-settings", false, crate::views::actions_settings::UserActionsSettingsPage, user_page);
    register_owner!("org/settings", "org-settings", true, crate::views::actions_settings::OrgActionsSettingsPage, org_page);
    Ok(())
}
async fn user_page(
    state: &crate::AppState,
    user: &rbit_core::User,
    _owner: &str,
    page: crate::views::layout::Page,
    notice: Option<Notice>,
) -> crate::error::WebResult<crate::views::actions_settings::UserActionsSettingsPage> {
    old::user_page(state, user, page, notice).await
}
async fn org_page(
    state: &crate::AppState,
    user: &rbit_core::User,
    owner: &str,
    page: crate::views::layout::Page,
    notice: Option<Notice>,
) -> crate::error::WebResult<crate::views::actions_settings::OrgActionsSettingsPage> {
    let ctx = old::managed_org(state, user, owner).await?;
    old::org_page(state, &ctx, page, notice).await
}
