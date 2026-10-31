//! Host-only browser identity and locale-cookie effects for declared preferences.

use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rbit_auth::{PluginBrowserToken, PluginCsrfBinding};
use rbit_services::plugin_pages::{LocaleChoice, MutationInputKind, MutationRouteHandle};

use crate::{
    AppState,
    extract::CurrentSession,
    middleware::{AccountLocale, LOCALE_COOKIE},
};

const SECURE_BROWSER_COOKIE: &str = "__Host-rbit-plugin-browser";
const LOCAL_BROWSER_COOKIE: &str = "rbit-plugin-browser";

pub(super) struct BrowserForms {
    jar: CookieJar,
    https: bool,
    selected: LocaleChoice,
    changed: bool,
}

impl BrowserForms {
    pub fn new(request: &Request, https: bool) -> Self {
        let jar = CookieJar::from_headers(request.headers());
        let selected = request
            .extensions()
            .get::<AccountLocale>()
            .and_then(|value| value.0.as_deref())
            .and_then(|value| LocaleChoice::new(value).ok())
            .or_else(|| jar.get(LOCALE_COOKIE).and_then(|cookie| LocaleChoice::new(cookie.value()).ok()))
            .unwrap_or(LocaleChoice::Automatic);
        Self { jar, https, selected, changed: false }
    }

    pub fn empty(https: bool) -> Self {
        Self { jar: CookieJar::new(), https, selected: LocaleChoice::Automatic, changed: false }
    }

    pub fn selected(&self) -> LocaleChoice {
        self.selected.clone()
    }

    pub fn issue(
        &mut self,
        state: &AppState,
        route: &MutationRouteHandle,
        binding: &PluginCsrfBinding<'_>,
        session: Option<&CurrentSession>,
        now: u64,
    ) -> Option<String> {
        if let Some(session) = session {
            return state.plugin_csrf.issue(&session.0, binding, now);
        }
        if route.descriptor().input_kind() != MutationInputKind::LocalePreference || binding.actor_id != 0 {
            return None;
        }
        let browser = self.browser().unwrap_or_else(|| {
            let browser = PluginBrowserToken::generate();
            self.jar = self.jar.clone().add(
                Cookie::build((browser_cookie_name(self.https), browser.encode()))
                    .path("/")
                    .http_only(true)
                    .secure(self.https)
                    .same_site(SameSite::Lax)
                    .max_age(time::Duration::days(1))
                    .build(),
            );
            self.changed = true;
            browser
        });
        state.plugin_csrf.issue_browser(&browser, binding, now)
    }

    pub fn verify(
        &self,
        state: &AppState,
        route: &MutationRouteHandle,
        binding: &PluginCsrfBinding<'_>,
        session: Option<&CurrentSession>,
        proof: &str,
        now: u64,
    ) -> bool {
        if let Some(session) = session {
            return state.plugin_csrf.verify(&session.0, binding, proof, now);
        }
        route.descriptor().input_kind() == MutationInputKind::LocalePreference
            && binding.actor_id == 0
            && self.browser().is_some_and(|browser| state.plugin_csrf.verify_browser(&browser, binding, proof, now))
    }

    fn browser(&self) -> Option<PluginBrowserToken> {
        self.jar.get(browser_cookie_name(self.https)).and_then(|cookie| PluginBrowserToken::parse(cookie.value()))
    }

    pub fn apply(self, response: Response) -> Response {
        if self.changed { (self.jar, response).into_response() } else { response }
    }

    pub fn choose(mut self, choice: &LocaleChoice, response: Response) -> Response {
        let cookie = Cookie::build((LOCALE_COOKIE, choice.preference().unwrap_or_default().to_owned()))
            .path("/")
            .same_site(SameSite::Lax)
            .secure(self.https)
            .http_only(true);
        self.jar = if choice.preference().is_some() {
            self.jar.add(cookie.max_age(time::Duration::days(365)).build())
        } else {
            self.jar.remove(cookie.build())
        };
        self.changed = true;
        self.apply(response)
    }
}

fn browser_cookie_name(https: bool) -> &'static str {
    if https { SECURE_BROWSER_COOKIE } else { LOCAL_BROWSER_COOKIE }
}
