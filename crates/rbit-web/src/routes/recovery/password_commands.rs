//! Protect submitted secrets before even an unpolled handler future owns them.

use super::{
    AppState, Form, FormError, IntoResponse, Page, Path, RESET_COMPLETE_URL, Redirect, ResetForm, ResetPasswordPage, Response,
    SECRET_URL_HEADERS, ServiceError, State, StatusCode, WebResult, problem_page,
};

/// `POST /password/reset/{token}`
pub fn reset(
    State(state): State<AppState>,
    page: Page,
    Path(token): Path<String>,
    Form(mut form): Form<ResetForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut replacement = zeroize::Zeroizing::new(std::mem::take(&mut form.new_password));
    let confirmation = zeroize::Zeroizing::new(std::mem::take(&mut form.confirm_password));
    async move {
        let recovery = &state.services.recovery;
        let result = if replacement.as_str() == confirmation.as_str() {
            recovery.reset_password(&token, std::mem::take(&mut *replacement)).await
        } else {
            Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
        };
        let response = match result {
            Ok(()) => Redirect::to(RESET_COMPLETE_URL).into_response(),
            Err(ServiceError::Invalid(message)) => {
                // Re-check the link: it may have been used or expired meanwhile.
                match recovery.reset_target(&token).await {
                    Ok(username) => {
                        let page = ResetPasswordPage::form(page, &token, username, state.config.security.min_password_length)
                            .with_error(FormError::from_message(&message));
                        (StatusCode::UNPROCESSABLE_ENTITY, page).into_response()
                    }
                    Err(error) => problem_page(&state, page, error),
                }
            }
            Err(error) => problem_page(&state, page, error),
        };
        Ok((SECRET_URL_HEADERS, response).into_response())
    }
}
