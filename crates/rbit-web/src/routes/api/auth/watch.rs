//! Refresh the caller of a long-running watch.

use std::sync::Arc;

use axum::http::HeaderValue;

use super::{ApiError, ApiResult, AppState, Caller, presented_token};

impl Caller {
    /// Recheck a principal during a bounded watch without charging another request.
    pub(in crate::routes::api) async fn refresh_watch(&mut self, state: &AppState, authorization: Option<&HeaderValue>) -> ApiResult<()> {
        if self.token.is_none() {
            if state.services.instance.require_sign_in() {
                return Err(ApiError::unauthorized("Requires authentication"));
            }
            return Ok(());
        }
        let secret = authorization.and_then(presented_token).ok_or_else(|| ApiError::unauthorized("Bad credentials"))?;
        let Some((user, scope, token_limit, token)) = state.services.tokens.authenticate_limited(secret).await? else {
            return Err(ApiError::unauthorized("Bad credentials"));
        };
        self.user = Some(Arc::new(user));
        self.scope = Some(scope);
        self.token_limit = token_limit;
        self.token = Some(token);
        Ok(())
    }
}
