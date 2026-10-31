//! Host-authorized POST transport for bounded, compiled plugin forms.

use axum::{body::Body, extract::State, http::HeaderValue};
use rbit_auth::PluginCsrfBinding;
use rbit_services::plugin_pages::{
    FormAction, FormActions, FormData, MutationAudience, MutationContext, MutationError, MutationRepository, MutationResponse,
    MutationRouteHandle,
};

use super::{
    form_input::{self, InputError, ParsedForm},
    *,
};
use crate::extract::CurrentSession;

fn now() -> Option<u64> {
    u64::try_from(rbit_core::Timestamp::now().seconds()).ok()
}

fn permitted(route: &MutationRouteHandle, viewer: &Viewer, repository: Option<&RepoPage>) -> bool {
    let Some(actor) = viewer.user() else { return false };
    match route.descriptor().audience() {
        MutationAudience::SignedIn => repository.is_none(),
        MutationAudience::SiteAdmin => repository.is_none() && actor.is_admin,
        MutationAudience::RepositoryWriter => repository.is_some_and(|repo| repo.ctx.can_write_content()),
        MutationAudience::RepositoryAdmin => repository.is_some_and(|repo| repo.ctx.access.can_admin()),
    }
}

fn action(route: &MutationRouteHandle, repository: Option<&RepoPage>) -> Option<String> {
    let owner = repository.map(|repo| repo.ctx.repo.owner.name.as_str());
    let name = repository.map(|repo| repo.ctx.repo.repo.name.as_str());
    route.descriptor().target(route.owner()).render(owner, name).ok()
}

fn binding<'a>(
    route: &MutationRouteHandle,
    actor: &rbit_core::User,
    repository: Option<&RepoPage>,
    action: &'a str,
) -> PluginCsrfBinding<'a> {
    PluginCsrfBinding {
        action,
        actor_id: actor.id().0,
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
) -> FormActions {
    if route.descriptor().forms().is_empty() {
        return FormActions::default();
    }
    let (Some(actor), Some(session), Some(now)) = (viewer.user(), session, now()) else { return FormActions::default() };
    let snapshot = state.services.mutation_registry().snapshot();
    let mut actions = Vec::new();
    for reference in route.descriptor().forms() {
        let Some(mutation) = snapshot.find(route.owner(), route.descriptor().scope(), &reference.path) else { continue };
        if mutation.generation() != route.generation() || !permitted(&mutation, viewer, repository) {
            continue;
        }
        let Some(action) = action(&mutation, repository) else { continue };
        let Some(proof) = state.plugin_csrf.issue(&session.0, &binding(&mutation, actor, repository, &action), now) else { continue };
        if let Ok(action) = FormAction::new(&reference.name, &action, &proof) {
            actions.push(action);
        }
    }
    FormActions::new(actions).unwrap_or_default()
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
    request: Request,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path().to_owned();
    let route = lookup_mutation(&state, &uri, &path.plugin, &path.page, PageScope::Global)?;
    if viewer.user().is_none() {
        return Ok(sign_in(&uri));
    }
    if !permitted(&route, &viewer, None) {
        return Err(WebError::NotFound);
    }
    submit(route, page, viewer, None, state, uri, request).await
}

pub(super) async fn repository(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<RepositoryPath>, PathRejection>,
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
    submit(route, page, viewer, Some(repository), state, uri, request).await
}

async fn submit(
    route: MutationRouteHandle,
    page: Page,
    viewer: Viewer,
    repository: Option<RepoPage>,
    state: AppState,
    uri: Uri,
    request: Request,
) -> WebResult<Response> {
    let Some(actor) = viewer.user() else { return Err(WebError::Forbidden) };
    let Some(session) = request.extensions().get::<CurrentSession>().cloned() else { return Err(WebError::Forbidden) };
    let Some(action) = action(&route, repository.as_ref()) else { return Err(WebError::NotFound) };
    if action != uri.path() {
        return Err(WebError::NotFound);
    }
    if !form_input::acceptable_origin(request.headers(), &state.config.server.public_url) {
        return Err(WebError::Forbidden);
    }
    if !form_input::supported_content_type(request.headers()) {
        return Ok(failure(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }
    if let Err(error) = form_input::check_length(request.headers()) {
        return Ok(failure(error.status()));
    }
    let Ok(lease) = route.admit() else { return Ok(mutation_unavailable()) };
    let parsed = match read(request.into_body(), &lease.cancellation_token()).await {
        Ok(parsed) => parsed,
        Err(StatusCode::SERVICE_UNAVAILABLE) => return Ok(mutation_unavailable()),
        Err(status) => return Ok(failure(status)),
    };
    let Some(now) = now() else { return Ok(mutation_unavailable()) };
    if !state.plugin_csrf.verify(&session.0, &binding(&route, actor, repository.as_ref(), &action), &parsed.proof, now) {
        return Err(WebError::Forbidden);
    }
    let (viewer, repository) = reauthorize(&state, &session, actor.id().0, repository, &route).await?;
    let actor = viewer.user().ok_or(WebError::Forbidden)?;
    let input = match FormData::new(parsed.fields) {
        Ok(input) => input,
        Err(_) => return Ok(failure(StatusCode::UNPROCESSABLE_ENTITY)),
    };
    let form = FormAction::new("submitted", &action, &parsed.proof).map_err(WebError::internal)?;
    let context = MutationContext::new(
        PageActor { account_id: actor.id().0, is_admin: actor.is_admin },
        repository.as_ref().map(|repo| MutationRepository {
            repository_id: repo.ctx.repo.repo.id.0,
            owner: repo.ctx.repo.owner.name.to_string(),
            name: repo.ctx.repo.repo.name.to_string(),
            can_write: repo.ctx.can_write_content(),
            can_admin: repo.ctx.access.can_admin(),
        }),
        page.locale.clone(),
        form,
    );
    let result = match lease.execute(context, input).await {
        Ok(result) => result,
        Err(MutationError::Forbidden) => return Err(WebError::NotFound),
        Err(MutationError::InvalidForm) => return Ok(failure(StatusCode::UNPROCESSABLE_ENTITY)),
        Err(_) => return Ok(mutation_unavailable()),
    };
    let (result, operation, cancellation) = result.into_parts();
    let mut response = match result {
        MutationResponse::Invalid(body) => {
            let mut response = shell(page, repository, route.owner().to_string(), route.descriptor().path().to_owned(), body, true, &state);
            *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
            response
        }
        MutationResponse::Redirect(target) => {
            // Literal same-plugin/scope paths cannot redirect to external sites or core recovery routes.
            let target = target.target(route.owner(), route.descriptor().scope());
            let target = target
                .render(
                    repository.as_ref().map(|repo| repo.ctx.repo.owner.name.as_str()),
                    repository.as_ref().map(|repo| repo.ctx.repo.repo.name.as_str()),
                )
                .map_err(WebError::internal)?;
            Redirect::to(&target).into_response()
        }
    };
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.extensions_mut().insert(crate::middleware::PageResponseGuard::mutation(operation, cancellation));
    Ok(response)
}

/// Upload time must not preserve grants that the host already knows have been revoked.
async fn reauthorize(
    state: &AppState,
    session: &CurrentSession,
    actor_id: i64,
    repository: Option<RepoPage>,
    route: &MutationRouteHandle,
) -> WebResult<(Viewer, Option<RepoPage>)> {
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
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "Plugin forms require URLencoded input.",
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
