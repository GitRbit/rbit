//! Protect submitted secrets before even an unpolled handler future owns them.

use super::{
    AppState, Client, CookieJar, Form, FormError, Https, IntoResponse, LoginForm, Page, Response, ServiceError, SignInOutcome, SignUp,
    SignupForm, State, StatusCode, ThrottleScope, WebResult, login_page, safe_redirect_target, second_factor_redirect, signup_page,
    start_session, throttled, too_many_requests,
};
use rbit_localization::shell::auth::{Attempt, AuthMessage};

pub fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(mut form): Form<LoginForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    async move {
        let next = safe_redirect_target(form.next.as_deref()).to_owned();
        let message = match state.services.sign_in(&form.login, std::mem::take(&mut *password), &client.0).await {
            Ok(SignInOutcome::SignedIn(user)) => return start_session(&state, jar, https, &client, &user, &next).await,
            Ok(SignInOutcome::SecondFactorRequired(pending)) => return Ok(second_factor_redirect(jar, https, &pending, &next)),
            Err(ServiceError::InvalidCredentials) => AuthMessage::IncorrectCredentials,
            Err(ServiceError::Forbidden) => AuthMessage::Suspended,
            Err(ServiceError::RateLimited { retry_after_seconds }) => {
                let error = throttled(&page, Attempt::SignIn, retry_after_seconds);
                let page = login_page(&state, page, error, form.login, next).await?;
                return Ok(too_many_requests(page, retry_after_seconds));
            }
            Err(other) => return Err(other.into()),
        };
        let error = FormError::localized(&page.text(message));
        let page = login_page(&state, page, error, form.login, next).await?;
        Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
    }
}

pub fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    https: Https,
    client: Client,
    page: Page,
    Form(mut form): Form<SignupForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    async move {
        let SignupForm { username, email, password: _, setup_token } = form;
        let ip = client.ip().map(|ip| ip.to_canonical().to_string()).unwrap_or_default();
        if let Err(ServiceError::RateLimited { retry_after_seconds }) = state.services.throttle.check(ThrottleScope::SignUp, &ip) {
            let error = throttled(&page, Attempt::SignUp, retry_after_seconds);
            let page = signup_page(&state, page, error, username, email, setup_token).await?;
            return Ok(too_many_requests(page, retry_after_seconds));
        }
        let input = SignUp { username: username.clone(), email: email.clone(), password: std::mem::take(&mut *password) };
        match state.services.accounts.sign_up(input, &setup_token).await {
            Ok(user) => start_session(&state, jar, https, &client, &user, "/").await,
            Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_) | ServiceError::RegistrationClosed)) => {
                let error = FormError::from_message(&error.to_string());
                let page = signup_page(&state, page, error, username, email, setup_token).await?;
                Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
            }
            Err(other) => Err(other.into()),
        }
    }
}
