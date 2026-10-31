//! Host-authorized POST transport for bounded, compiled plugin forms.

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue},
};
use rbit_auth::PluginCsrfBinding;
use rbit_services::plugin_pages::{
    FormAction, FormActions, FormData, MutationAudience, MutationContext, MutationError, MutationInputKind, MutationRepository,
    MutationResponse, MutationRouteHandle, UploadData,
};

use super::{
    AppState, GlobalPath, IntoResponse, OriginalUri, Page, PageActor, PageRouteHandle, PageScope, Path, PathRejection, PluginId, Redirect,
    RepoPage, RepositoryPath, Request, Response, ShellPolicy, StatusCode, Uri, Viewer, WebError, WebResult,
    browser_preference::BrowserForms,
    form_input::{self, InputError, ParsedForm},
    header, shell, sign_in, validate_page_path,
};
use crate::{client_ip::Https, extract::CurrentSession};

struct BrowserRequest {
    request: Request,
    browser: BrowserForms,
}

enum PendingInput {
    Fields(Vec<(String, String)>),
    Upload(UploadData),
}

struct Submission {
    proof: String,
    input: PendingInput,
}

fn now() -> Option<u64> {
    u64::try_from(rbit_core::Timestamp::now().seconds()).ok()
}

fn permitted(route: &MutationRouteHandle, viewer: &Viewer, repository: Option<&RepoPage>) -> bool {
    if route.descriptor().audience() == MutationAudience::LocalePreference {
        return repository.is_none() && route.descriptor().input_kind() == MutationInputKind::LocalePreference;
    }
    let Some(actor) = viewer.user() else { return false };
    match route.descriptor().audience() {
        MutationAudience::SignedIn => repository.is_none(),
        MutationAudience::SiteAdmin => repository.is_none() && actor.is_admin,
        MutationAudience::RepositoryWriter => repository.is_some_and(|repo| repo.ctx.can_write_content()),
        MutationAudience::RepositoryAdmin => repository.is_some_and(|repo| repo.ctx.access.can_admin()),
        MutationAudience::LocalePreference => false,
    }
}

fn action(route: &MutationRouteHandle, repository: Option<&RepoPage>) -> Option<String> {
    let owner = repository.map(|repo| repo.ctx.repo.owner.name.as_str());
    let name = repository.map(|repo| repo.ctx.repo.repo.name.as_str());
    route.descriptor().target(route.owner()).render(owner, name).ok()
}

fn binding<'a>(
    route: &MutationRouteHandle,
    actor: Option<&rbit_core::User>,
    repository: Option<&RepoPage>,
    action: &'a str,
) -> PluginCsrfBinding<'a> {
    PluginCsrfBinding {
        action,
        actor_id: actor.map_or(0, |actor| actor.id().0),
        repository_id: repository.map(|repo| repo.ctx.repo.repo.id.0),
        generation: route.generation().get(),
        version: route.version(),
    }
}

/// Only explicitly declared same-generation, same-owner and same-scope forms receive proofs.
pub(super) fn actions(
    route: &PageRouteHandle,
    viewer: &Viewer,
    session: Option<&CurrentSession>,
    repository: Option<&RepoPage>,
    state: &AppState,
    browser: &mut BrowserForms,
) -> FormActions {
    if route.descriptor().forms().is_empty() {
        return FormActions::default();
    }
    let Some(now) = now() else { return FormActions::default() };
    let snapshot = state.services.mutation_registry().snapshot();
    let mut actions = Vec::new();
    let mut has_preference = false;
    for reference in route.descriptor().forms() {
        let Some(mutation) = snapshot.find(route.owner(), route.descriptor().scope(), &reference.path) else { continue };
        if mutation.generation() != route.generation() || !permitted(&mutation, viewer, repository) {
            continue;
        }
        let Some(action) = action(&mutation, repository) else { continue };
        let Some(proof) = browser.issue(state, &mutation, &binding(&mutation, viewer.user(), repository, &action), session, now) else {
            continue;
        };
        if let Ok(action) = FormAction::new(&reference.name, &action, &proof) {
            has_preference |= mutation.descriptor().input_kind() == MutationInputKind::LocalePreference;
            actions.push(action);
        }
    }
    let mut actions = FormActions::new(actions).unwrap_or_default();
    if has_preference {
        actions.set_locale_preference(browser.selected());
    }
    actions
}

fn lookup_mutation(state: &AppState, uri: &Uri, owner: &str, path: &str, scope: PageScope) -> WebResult<MutationRouteHandle> {
    if uri.path().contains('%') || !validate_page_path(path) {
        return Err(WebError::NotFound);
    }
    let owner: PluginId = owner.parse().map_err(|_| WebError::NotFound)?;
    state.services.mutation_registry().snapshot().find(&owner, scope, path).ok_or(WebError::NotFound)
}

pub(super) async fn global(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<GlobalPath>, PathRejection>,
    Https(https): Https,
    request: Request,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path().to_owned();
    let route = lookup_mutation(&state, &uri, &path.plugin, &path.page, PageScope::Global)?;
    if viewer.user().is_none() && route.descriptor().input_kind() != MutationInputKind::LocalePreference {
        return Ok(sign_in(&uri));
    }
    if !permitted(&route, &viewer, None) {
        return Err(WebError::NotFound);
    }
    let browser = if route.descriptor().input_kind() == MutationInputKind::LocalePreference {
        BrowserForms::new(&request, https)
    } else {
        BrowserForms::empty(https)
    };
    submit(route, page, viewer, None, state, uri, BrowserRequest { request, browser }).await
}

pub(super) async fn repository(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<RepositoryPath>, PathRejection>,
    Https(https): Https,
    request: Request,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path().to_owned();
    let route = lookup_mutation(&state, &uri, &path.plugin, &path.page, PageScope::Repository)?;
    let repository = RepoPage::load(&state, &viewer, &path.owner, &path.repo).await?;
    if viewer.user().is_none() {
        return Ok(sign_in(&uri));
    }
    if !permitted(&route, &viewer, Some(&repository)) {
        return Err(WebError::NotFound);
    }
    let browser = BrowserForms::empty(https);
    submit(route, page, viewer, Some(repository), state, uri, BrowserRequest { request, browser }).await
}

async fn submit(
    route: MutationRouteHandle,
    page: Page,
    viewer: Viewer,
    repository: Option<RepoPage>,
    state: AppState,
    uri: Uri,
    BrowserRequest { request, browser }: BrowserRequest,
) -> WebResult<Response> {
    let actor_id = viewer.user().map_or(0, |actor| actor.id().0);
    let session = request.extensions().get::<CurrentSession>().cloned();
    if actor_id > 0 && session.is_none() {
        return Err(WebError::Forbidden);
    }
    let Some(action) = action(&route, repository.as_ref()) else { return Err(WebError::NotFound) };
    if action != uri.path() {
        return Err(WebError::NotFound);
    }
    if !form_input::acceptable_origin(request.headers(), &state.config.server.public_url) {
        return Err(WebError::Forbidden);
    }
    if let Err(status) = check_input_headers(route.descriptor().input_kind(), request.headers()) {
        return Ok(failure(status));
    }
    let Ok(lease) = route.admit() else { return Ok(mutation_unavailable()) };
    let parsed = match read_submission(request, route.descriptor().input_kind(), &lease.cancellation_token()).await {
        Ok(parsed) => parsed,
        Err(StatusCode::SERVICE_UNAVAILABLE) => return Ok(mutation_unavailable()),
        Err(status) => return Ok(failure(status)),
    };
    let Some(now) = now() else { return Ok(mutation_unavailable()) };
    if !browser.verify(&state, &route, &binding(&route, viewer.user(), repository.as_ref(), &action), session.as_ref(), &parsed.proof, now)
    {
        return Err(WebError::Forbidden);
    }
    let (viewer, repository) = reauthorize(&state, session.as_ref(), actor_id, repository, &route).await?;
    let form = FormAction::new("submitted", &action, &parsed.proof).map_err(WebError::internal)?;
    let context = mutation_context(&route, &viewer, repository.as_ref(), &page, form)?;
    let executed = match parsed.input {
        PendingInput::Fields(fields) => {
            let Ok(input) = FormData::new(fields) else { return Ok(failure(StatusCode::UNPROCESSABLE_ENTITY)) };
            lease.execute(context, input).await
        }
        PendingInput::Upload(input) => lease.execute_upload(context, input).await,
    };
    let result = match executed {
        Ok(result) => result,
        Err(MutationError::Forbidden) => return Err(WebError::NotFound),
        Err(MutationError::InvalidForm) => return Ok(failure(StatusCode::UNPROCESSABLE_ENTITY)),
        Err(_) => return Ok(mutation_unavailable()),
    };
    render_result(result, page, repository, &route, &state, browser)
}

fn mutation_context(
    route: &MutationRouteHandle,
    viewer: &Viewer,
    repository: Option<&RepoPage>,
    page: &Page,
    form: FormAction,
) -> WebResult<MutationContext> {
    let actor = viewer.user().map(|actor| PageActor { account_id: actor.id().0, is_admin: actor.is_admin });
    let context = if route.descriptor().input_kind() == MutationInputKind::LocalePreference {
        MutationContext::for_preference(actor, page.locale.clone(), form)
    } else {
        MutationContext::new(
            actor.ok_or(WebError::Forbidden)?,
            repository.map(|repo| MutationRepository {
                repository_id: repo.ctx.repo.repo.id.0,
                owner: repo.ctx.repo.owner.name.to_string(),
                name: repo.ctx.repo.repo.name.to_string(),
                can_write: repo.ctx.can_write_content(),
                can_admin: repo.ctx.access.can_admin(),
            }),
            page.locale.clone(),
            form,
        )
    };
    Ok(context)
}

fn render_result(
    result: rbit_services::plugin_pages::MutationExecution,
    page: Page,
    repository: Option<RepoPage>,
    route: &MutationRouteHandle,
    state: &AppState,
    browser: BrowserForms,
) -> WebResult<Response> {
    let (result, operation, cancellation) = result.into_parts();
    let (mut response, choice) = match result {
        MutationResponse::Invalid(body) => {
            let policy = ShellPolicy { native_forms: true, administrator: route.descriptor().audience() == MutationAudience::SiteAdmin };
            let mut response =
                shell(page, repository, route.owner().to_string(), route.descriptor().path().to_owned(), body, policy, state);
            *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
            (response, None)
        }
        MutationResponse::Redirect(target) => (redirect_to(&target, route, repository.as_ref())?, None),
        MutationResponse::LocalePreference { choice, redirect } => {
            if route.descriptor().input_kind() != MutationInputKind::LocalePreference {
                return Ok(mutation_unavailable());
            }
            (redirect_to(&redirect, route, repository.as_ref())?, Some(choice))
        }
    };
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.extensions_mut().insert(crate::middleware::PageResponseGuard::mutation(operation, cancellation));
    Ok(if let Some(choice) = choice { browser.choose(&choice, response) } else { response })
}

fn redirect_to(
    target: &rbit_services::plugin_pages::PluginRedirect,
    route: &MutationRouteHandle,
    repository: Option<&RepoPage>,
) -> WebResult<Response> {
    // Literal same-plugin/scope paths cannot redirect to external sites or core recovery routes.
    let target = target.target(route.owner(), route.descriptor().scope());
    let target = target
        .render(repository.map(|repo| repo.ctx.repo.owner.name.as_str()), repository.map(|repo| repo.ctx.repo.repo.name.as_str()))
        .map_err(WebError::internal)?;
    Ok(Redirect::to(&target).into_response())
}

/// Upload time must not preserve grants that the host already knows have been revoked.
async fn reauthorize(
    state: &AppState,
    session: Option<&CurrentSession>,
    actor_id: i64,
    repository: Option<RepoPage>,
    route: &MutationRouteHandle,
) -> WebResult<(Viewer, Option<RepoPage>)> {
    let Some(session) = session else {
        return if actor_id == 0 && repository.is_none() && route.descriptor().input_kind() == MutationInputKind::LocalePreference {
            Ok((Viewer(None), None))
        } else {
            Err(WebError::Forbidden)
        };
    };
    let signed_in = state.services.accounts.resume_session(&session.0).await?.ok_or(WebError::Forbidden)?;
    if signed_in.password_change_required || signed_in.user.id().0 != actor_id {
        return Err(WebError::Forbidden);
    }
    let viewer = Viewer(Some(std::sync::Arc::new(signed_in.user)));
    let repository = if let Some(old) = repository {
        let fresh = RepoPage::load(state, &viewer, old.ctx.repo.owner.name.as_str(), old.ctx.repo.repo.name.as_str()).await?;
        if fresh.ctx.repo.repo.id != old.ctx.repo.repo.id {
            return Err(WebError::Forbidden);
        }
        Some(fresh)
    } else {
        None
    };
    if !permitted(route, &viewer, repository.as_ref()) {
        return Err(WebError::NotFound);
    }
    Ok((viewer, repository))
}

fn check_input_headers(kind: MutationInputKind, headers: &HeaderMap) -> Result<(), StatusCode> {
    if let MutationInputKind::Upload { max_bytes } = kind {
        return super::upload_input::validate_headers(headers, max_bytes);
    }
    if !form_input::supported_content_type(headers) {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    form_input::check_length(headers).map_err(InputError::status)
}

async fn read_submission(
    request: Request,
    kind: MutationInputKind,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<Submission, StatusCode> {
    let (parts, body) = request.into_parts();
    if let MutationInputKind::Upload { max_bytes } = kind {
        let (proof, input) = super::upload_input::parse(body, &parts.headers, cancellation, max_bytes).await?;
        return Ok(Submission { proof, input: PendingInput::Upload(input) });
    }
    let parsed = read(body, cancellation).await?;
    Ok(Submission { proof: parsed.proof, input: PendingInput::Fields(parsed.fields) })
}

async fn read(mut body: Body, cancellation: &tokio_util::sync::CancellationToken) -> Result<ParsedForm, StatusCode> {
    use http_body::Body as _;
    let mut bytes = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let frame = tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(StatusCode::SERVICE_UNAVAILABLE),
            () = tokio::time::sleep_until(deadline) => return Err(StatusCode::REQUEST_TIMEOUT),
            frame = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)) => frame,
        };
        let Some(frame) = frame else { break };
        let frame = frame.map_err(|_| StatusCode::BAD_REQUEST)?;
        let data = frame.into_data().map_err(|_| StatusCode::BAD_REQUEST)?;
        if data.len() > form_input::MAX_ENCODED_FORM_BYTES.saturating_sub(bytes.len()) {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        bytes.extend_from_slice(&data);
    }
    ParsedForm::decode(&bytes).map_err(InputError::status)
}

fn failure(status: StatusCode) -> Response {
    let message = match status {
        StatusCode::FORBIDDEN => "Invalid plugin form proof.",
        StatusCode::PAYLOAD_TOO_LARGE => "Plugin form is too large.",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "Unsupported plugin form encoding.",
        StatusCode::UNPROCESSABLE_ENTITY => "Invalid plugin form.",
        StatusCode::REQUEST_TIMEOUT => "Plugin form upload timed out.",
        StatusCode::SERVICE_UNAVAILABLE => "Plugin mutation outcome is unavailable; do not retry automatically.",
        _ => "Malformed plugin form.",
    };
    let mut response = (status, [(header::CACHE_CONTROL, "no-store")]).into_response();
    response.extensions_mut().insert(crate::error::ErrorInfo { status, title: "Bad request", message: message.into() });
    response
}

fn mutation_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CACHE_CONTROL, "no-store")],
        "Plugin mutation outcome is unknown. Do not retry automatically.",
    )
        .into_response()
}
