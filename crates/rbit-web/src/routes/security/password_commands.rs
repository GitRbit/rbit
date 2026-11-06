//! Protect submitted secrets before even an unpolled handler future owns them.

use super::*;

/// `POST /settings/security/two-factor/disable`
pub fn disable(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(mut form): Form<PasswordAndCodeForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    let code = zeroize::Zeroizing::new(std::mem::take(&mut form.code));
    let passkey = zeroize::Zeroizing::new(std::mem::take(&mut form.passkey));
    async move {
        match state.services.disable_two_factor(&user, std::mem::take(&mut *password), proof(&code, &passkey), &client.0).await {
            Ok(()) => Ok(Redirect::to(&SecurityNotice::TwoFactorDisabled.url()).into_response()),
            Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::Disable, error).await,
        }
    }
}

/// `POST /settings/security/recovery-codes`: replaces the codes and shows them once.
pub fn regenerate_codes(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(mut form): Form<PasswordAndCodeForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    let code = zeroize::Zeroizing::new(std::mem::take(&mut form.code));
    let passkey = zeroize::Zeroizing::new(std::mem::take(&mut form.passkey));
    async move {
        match state.services.regenerate_recovery_codes(&user, std::mem::take(&mut *password), proof(&code, &passkey), &client.0).await {
            Ok(codes) => Ok(no_store(SecurityPage::new(page, SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: false })))),
            Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::Regenerate, error).await,
        }
    }
}

/// `POST /settings/security/passkeys/{id}/remove`
pub fn remove_passkey(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Path(id): Path<i64>,
    Form(mut form): Form<PasswordAndCodeForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    let code = zeroize::Zeroizing::new(std::mem::take(&mut form.code));
    let passkey = zeroize::Zeroizing::new(std::mem::take(&mut form.passkey));
    async move {
        match state.services.remove_passkey(&user, id, std::mem::take(&mut *password), proof(&code, &passkey), &client.0).await {
            Ok(()) => Ok(Redirect::to(&SecurityNotice::PasskeyRemoved.url()).into_response()),
            Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::RemovePasskey(id), error).await,
        }
    }
}

/// `POST /settings/security/two-factor`: confirms enrolment and, for a first factor, shows
/// the recovery codes once (not a redirect: they are never stored in a readable form).
pub fn confirm_setup(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    client: Client,
    page: Page,
    Form(mut form): Form<CodeForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let code = zeroize::Zeroizing::new(std::mem::take(&mut form.code));
    async move {
        let (status, message) = match state.services.confirm_two_factor(&user, &code, &client.0).await {
            Ok(Some(codes)) => {
                let view = SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: true });
                return Ok(no_store(SecurityPage::new(page, view)));
            }
            Ok(None) => return Ok(Redirect::to(&SecurityNotice::AuthenticatorAdded.url()).into_response()),
            Err(ServiceError::InvalidCredentials) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "That code didn't match. Check the time on your device and try the next code.".to_owned(),
            ),
            Err(ServiceError::RateLimited { retry_after_seconds }) => {
                (StatusCode::TOO_MANY_REQUESTS, slow_down("code", retry_after_seconds))
            }
            Err(ServiceError::Invalid(message)) => {
                (StatusCode::UNPROCESSABLE_ENTITY, FormError::from_message(&message).0.unwrap_or_default())
            }
            Err(other) => return Err(other.into()),
        };
        let enrolment = state.services.two_factor.begin_enrolment(&user, &page.site_name).await?;
        let page = SecurityPage::new(page, SecurityView::Setup(Setup::new(&enrolment, FormError::new(message))));
        Ok((status, no_store(page)).into_response())
    }
}

/// `POST /settings/security/passkeys`: registers the passkey the browser just created. The
/// account's first factor shows its new recovery codes once.
pub fn add_passkey(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    current: CurrentSession,
    client: Client,
    page: Page,
    Form(mut form): Form<AddPasskeyForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let credential = zeroize::Zeroizing::new(std::mem::take(&mut form.credential));
    async move {
        if credential.trim().is_empty() {
            let error = ServiceError::Invalid("your browser didn't send a passkey; try again".to_owned());
            return overview_with_error(&state, &user, &current, page, SensitiveForm::AddPasskey, error).await;
        }
        match state.services.add_passkey(&user, &form.name, &credential, &client.0).await {
            Ok(PasskeyRegistered::FirstFactor(codes)) => {
                Ok(no_store(SecurityPage::new(page, SecurityView::RecoveryCodes(RecoveryCodes { codes, just_enabled: true }))))
            }
            Ok(PasskeyRegistered::Added) => Ok(Redirect::to(&SecurityNotice::PasskeyAdded.url()).into_response()),
            Err(error) => overview_with_error(&state, &user, &current, page, SensitiveForm::AddPasskey, error).await,
        }
    }
}

/// `POST /login/two-factor`
pub fn two_factor(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(mut form): Form<TwoFactorForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let code = zeroize::Zeroizing::new(std::mem::take(&mut form.code));
    let passkey = zeroize::Zeroizing::new(std::mem::take(&mut form.passkey));
    async move {
        let next = safe_redirect_target(form.next.as_deref()).to_owned();
        let Some(pending) = pending_token(&jar) else {
            return Ok(Redirect::to("/login").into_response());
        };
        let used_passkey = !passkey.trim().is_empty();
        match state.services.complete_sign_in(&pending, proof(&code, &passkey), &client.0).await {
            Ok(user) => start_session(&state, without_pending(jar), https, &client, &user, &next).await,
            Err(ServiceError::InvalidCredentials) => {
                let error = FormError::new(if used_passkey {
                    "That passkey didn't work. Use one registered to this account, or enter a code."
                } else {
                    "That code didn't work. Check your authenticator app and try again."
                });
                let page = second_step_page(&state, &pending, page, error, next).await?;
                Ok((StatusCode::UNPROCESSABLE_ENTITY, no_store(page)).into_response())
            }
            Err(ServiceError::RateLimited { retry_after_seconds }) => {
                let error = FormError::new(slow_down("code", retry_after_seconds));
                Ok(too_many_requests(second_step_page(&state, &pending, page, error, next).await?, retry_after_seconds))
            }
            Err(ServiceError::NotFound) => {
                let error = FormError::new("Your sign-in expired. Enter your password again.");
                let login = login_page(&state, page.titled("Two-factor authentication"), error, String::new(), next).await?;
                Ok((without_pending(jar), login).into_response())
            }
            Err(other) => Err(other.into()),
        }
    }
}
