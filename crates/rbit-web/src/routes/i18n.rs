//! Legacy bookmark compatibility; language functionality is a scoped native route.

use crate::{
    AppState,
    error::{WebError, WebResult},
};
use axum::{Router, extract::State, response::Redirect, routing::get};

pub fn routes() -> Router<AppState> {
    Router::new().route("/language", get(index))
}

async fn index(State(state): State<AppState>) -> WebResult<Redirect> {
    if !state.services.i18n_enabled() {
        return Err(WebError::NotFound);
    }
    Ok(Redirect::to("/-/plugins/rbit.i18n/language"))
}
