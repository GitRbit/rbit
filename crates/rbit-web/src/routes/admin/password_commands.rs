//! Protect submitted secrets before even an unpolled handler future owns them.

use super::{
    AccountId, AdminSection, AppState, Form, FormError, IntoResponse, NewUserForm, Page, Path, Redirect, Response, ServiceError,
    SetPasswordForm, SignUp, SiteAdmin, State, StatusCode, UsersNotice, WebResult, listing_url, new_user_page, parse_id, set_password_page,
};

/// `POST /admin/users/{id}/password`
pub fn set_password(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Path(id): Path<String>,
    Form(mut form): Form<SetPasswordForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut replacement = zeroize::Zeroizing::new(std::mem::take(&mut form.new_password));
    let confirmation = zeroize::Zeroizing::new(std::mem::take(&mut form.confirm_password));
    async move {
        let record = state.services.admin.user(&admin, AccountId(parse_id(&id)?)).await?;
        let result = if replacement.as_str() == confirmation.as_str() {
            state.services.recovery.admin_set_password(&admin, record.user.id(), std::mem::take(&mut *replacement)).await
        } else {
            Err(ServiceError::Invalid("the new passwords do not match".to_owned()))
        };
        match result {
            Ok(()) => {
                let listing = listing_url(AdminSection::Users.url(), record.user.name().as_str(), 1);
                Ok(Redirect::to(&UsersNotice::PasswordSet.url(&listing)).into_response())
            }
            Err(ServiceError::Invalid(message)) => {
                let page = set_password_page(&state, &admin, page, &record).with_error(FormError::from_message(&message));
                Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
            }
            Err(other) => Err(other.into()),
        }
    }
}

/// `POST /admin/users/new`
pub fn create_user(
    State(state): State<AppState>,
    SiteAdmin(admin): SiteAdmin,
    page: Page,
    Form(mut form): Form<NewUserForm>,
) -> impl std::future::Future<Output = WebResult<Response>> {
    let mut password = zeroize::Zeroizing::new(std::mem::take(&mut form.password));
    async move {
        let is_admin = form.is_admin.is_some();
        let input = SignUp { username: form.username.clone(), email: form.email.clone(), password: std::mem::take(&mut *password) };
        match state.services.admin.create_user(&admin, input, is_admin).await {
            Ok(user) => Ok(Redirect::to(&listing_url(AdminSection::Users.url(), user.name().as_str(), 1)).into_response()),
            Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
                let page = new_user_page(&state, page, FormError::from_message(&error.to_string()), form.username, form.email, is_admin);
                Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
            }
            Err(other) => Err(other.into()),
        }
    }
}
