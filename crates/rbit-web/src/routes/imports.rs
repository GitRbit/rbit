//! Importing from GitHub and Gitea: `/new/import` and `/{owner}/{repo}/import`.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. The progress page is for the importer and the
//! repository's administrators; everyone else gets 404. The token is never put back into
//! the form.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_core::{
    User, Visibility,
    import::{ImportOptions, SourceKind},
};
use rbit_services::{ServiceError, imports::ImportInput};
use serde::Deserialize;

use super::{repo::RepoPage, repo_create::owner_options};
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        imports::{ImportStatusPage, NewImportPage, OptionChoice, SourceChoice},
        layout::Page,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/new/import", get(new_form).post(create)).route("/{owner}/{repo}/import", get(status))
}

#[derive(Debug, Deserialize)]
pub struct NewImportForm {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    name: String,
    visibility: Option<String>,
    issues: Option<String>,
    labels: Option<String>,
    milestones: Option<String>,
    releases: Option<String>,
    lfs: Option<String>,
}

impl NewImportForm {
    fn options(&self) -> ImportOptions {
        ImportOptions {
            issues: self.issues.is_some(),
            labels: self.labels.is_some(),
            milestones: self.milestones.is_some(),
            releases: self.releases.is_some(),
            lfs: self.lfs.is_some(),
        }
    }

    fn kind(&self) -> SourceKind {
        SourceKind::parse(&self.kind).unwrap_or_default()
    }
}

fn option_choices(options: ImportOptions) -> Vec<OptionChoice> {
    vec![
        OptionChoice {
            field: "issues",
            label: "Issues and pull requests",
            description: "With their comments, numbers and dates. Pull requests become history; open ones stay open when their commits came along.",
            checked: options.issues,
        },
        OptionChoice { field: "labels", label: "Labels", description: "Names, colours and descriptions.", checked: options.labels },
        OptionChoice {
            field: "milestones",
            label: "Milestones",
            description: "Open and closed, with due dates.",
            checked: options.milestones,
        },
        OptionChoice { field: "releases", label: "Releases", description: "Notes and attached files.", checked: options.releases },
        OptionChoice {
            field: "lfs",
            label: "Git LFS files",
            description: "Large files stored outside the repository's history.",
            checked: options.lfs,
        },
    ]
}

async fn form_page(
    state: &AppState,
    user: &User,
    page: Page,
    form: &NewImportForm,
    options: ImportOptions,
    error: FormError,
) -> WebResult<NewImportPage> {
    let default_private = !state.services.instance.current().default_visibility.is_public();
    Ok(NewImportPage {
        page: page.titled(crate::i18n::text("core", "title-import-a-repository")),
        error,
        disabled: !state.services.imports.is_enabled(),
        sources: SourceChoice::all(form.kind()),
        host: form.host.clone(),
        repository: form.repository.clone(),
        owners: owner_options(state, user, &form.owner).await?,
        name: form.name.clone(),
        private: form.visibility.as_deref().map_or(default_private, |v| v == Visibility::Private.as_str()),
        options: option_choices(options),
    })
}

/// `GET /new/import`
async fn new_form(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<NewImportPage> {
    let form = NewImportForm {
        kind: String::new(),
        host: String::new(),
        repository: String::new(),
        token: String::new(),
        owner: String::new(),
        name: String::new(),
        visibility: None,
        issues: None,
        labels: None,
        milestones: None,
        releases: None,
        lfs: None,
    };
    form_page(&state, &user, page, &form, ImportOptions::default(), FormError::default()).await
}

/// `POST /new/import`: validates the source, creates the repository and redirects to the
/// import's progress page.
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(form): Form<NewImportForm>,
) -> WebResult<Response> {
    let visibility =
        form.visibility.as_deref().and_then(Visibility::parse).unwrap_or_else(|| state.services.instance.current().default_visibility);
    let input = ImportInput {
        kind: form.kind(),
        host: form.host.clone(),
        repository: form.repository.clone(),
        token: form.token.clone(),
        owner: form.owner.clone(),
        name: form.name.clone(),
        visibility,
        options: form.options(),
    };
    match state.services.imports.start(&user, input).await {
        Ok(created) => Ok(Redirect::to(&format!("/{}/{}/import", created.owner.name, created.repo.name)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let view = form_page(&state, &user, page, &form, form.options(), FormError::from_message(&error.to_string())).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /{owner}/{repo}/import`: the import's progress (refreshes itself while it runs).
async fn status(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<ImportStatusPage> {
    let user = viewer.user().ok_or(WebError::NotFound)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let import = state.services.imports.status(user, &loaded.ctx).await?;
    let (status_label, status_class) = ImportStatusPage::status_badge(import.status);
    Ok(ImportStatusPage {
        page: page.titled(format!("Import · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "code"),
        source_label: import.kind.label(),
        status_label,
        status_class,
        active: import.status.is_active(),
        phases: ImportStatusPage::phases(&import),
        warnings: import.warnings.clone(),
        dropped_warnings: import.counts.dropped_warnings,
        error: import.error.as_deref().map(|error| {
            let mut sentence = error.trim_end_matches('.').to_owned();
            if let Some(first) = sentence.get(..1) {
                sentence.replace_range(..1, &first.to_uppercase());
            }
            format!("{sentence}.")
        }),
        started: import.started_at.map(|at| (at.relative(), at.iso8601())),
        finished: import.finished_at.map(|at| (at.relative(), at.iso8601())),
        self_url: format!("{}/import", loaded.urls.home()),
        source_url: import.source_url,
    })
}
