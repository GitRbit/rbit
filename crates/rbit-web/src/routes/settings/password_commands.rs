//! Protect submitted secrets before even an unpolled handler future owns them.

use super::*;

/// `POST /settings/account/username`: changes only the signed-in account's name.
pub fn change_username(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(mut form): Form<UsernameFormInput>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.current_password));
    async move {
        let result = state.services.profiles.change_username(&user, std::mem::take(&mut *password), &form.username).await;
        account_result(&state, &user, page, AccountForm::Username, result, Some(form.username)).await
    }
}

/// `POST /settings/account/email`
pub fn change_email(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(mut form): Form<EmailFormInput>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.current_password));
    async move {
        let result = state.services.profiles.change_email(&user, std::mem::take(&mut *password), &form.email).await;
        account_result(&state, &user, page, AccountForm::Email, result.map(drop), Some(form.email)).await
    }
}

/// `POST /settings/account/password`
pub fn change_password(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(mut form): Form<PasswordFormInput>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut current = zeroize::Zeroizing::new(std::mem::take(&mut form.current));
    let mut replacement = zeroize::Zeroizing::new(std::mem::take(&mut form.new));
    let confirmation = zeroize::Zeroizing::new(std::mem::take(&mut form.confirmation));
    async move {
        let result = if replacement.as_str() == confirmation.as_str() {
            state.services.profiles.change_password(&user, std::mem::take(&mut *current), std::mem::take(&mut *replacement)).await
        } else {
            Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
        };
        account_result(&state, &user, page, AccountForm::Password, result, None).await
    }
}
