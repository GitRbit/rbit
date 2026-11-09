//! Signing in with external identity providers: OIDC and GitHub OAuth,
//! configured as `[[auth.providers]]`.
//!
//! Milestone 3 workstream O (external sign-in) owns this file and `external_auth/`.
//!
//! **The flow.** [`ExternalAuthService::begin`] seals a [`Flow`] (provider, random
//! `state`, where to go next, and for a link the account linking) into a cookie with a
//! key kept in `server_secrets`, and returns the provider's authorization URL with that
//! `state`, a `nonce` and a PKCE `S256` challenge, both derived from the `state` with the
//! key (so nothing is stored server-side). [`ExternalAuthService::callback`] opens the
//! cookie, requires the returned `state` to match exactly, exchanges the code with the
//! PKCE verifier and turns the answer into a [`VerifiedIdentity`]: for OIDC from the ID
//! token, verified against the provider's keys (fetched again once when its `kid` is
//! unknown, so key rotation just works), falling back to the userinfo endpoint for the
//! address; for GitHub from `/user` and `/user/emails` (the verified primary address
//! only). Callbacks are throttled per client address ([`ThrottleScope::ExternalSignIn`]);
//! failures cost extra and are audited.
//!
//! The callback checks, linking, registration and sign-in decisions are
//! `rbit_identity::external`'s; this module runs the provider exchange and the adapters.
//!
//! **Linking** ([`Services::complete_external_sign_in`]), for an identity — always the
//! provider's stable subject, never an address or login name:
//!
//! 1. already linked: sign in as its account (refused while suspended);
//! 2. unknown, and the flow is a link started from Settings → Security by a signed-in
//!    user: link it to that account;
//! 3. unknown, the provider has `trust_email`, its address is verified and equals the
//!    verified address of an account: link it to that account and sign in;
//! 4. unknown, and the provider may register accounts (`allow_registration`: `true`
//!    follows the site's registration setting, `"always"` ignores it, `false` never; and
//!    never before the instance has its first, administrator account):
//!    continue at a "choose a username" step ([`Services::register_external`]), whose
//!    address is marked verified only if it is the provider's verified one;
//! 5. otherwise refuse, explaining how to link from settings.
//!
//! An unverified address never links anything. Signing in through a provider satisfies
//! the password step only: accounts with two-factor authentication continue at the
//! second step, unless the provider has `trust_mfa` and its ID token reports `mfa` in
//! `amr`. Accounts registered through a provider have no password (an empty hash, which
//! never verifies) until they set one with a reset link; Git over HTTPS then needs an
//! access token. An identity can't be unlinked while it is the account's only way to
//! sign in (no password, no other identity at a configured provider).

mod callback_mode;
pub mod client;
mod links;
pub use callback_mode::OriginalProviderCallback;
pub use links::OriginalLinkCallback;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rbit_auth::{Flow, FlowKey, IdTokenCheck, JwkSet, OidcError, SessionToken, oidc};
use rbit_core::{
    AccountId, Timestamp, User,
    config::{ProviderConfig, ProviderKind, ProviderRegistration},
};
#[cfg(test)]
use rbit_identity::external::suggest_username;
use rbit_identity::external::{
    LAST_METHOD, ProviderPolicy, callback,
    callback::Refusal,
    is_last_method,
    pending::PendingRegistrations,
    provider::{self, apply_userinfo, needs_userinfo},
    suggested_username, vouched_email,
};
pub use rbit_identity::external::{
    VerifiedIdentity,
    callback::{CallbackParams, FLOW_TTL_SECONDS},
    pending::REGISTRATION_TTL,
};
use rbit_identity_models::sessions::ClientInfo;
use rbit_storage::{
    Database, NewUser, StorageError,
    sqlite::external_identities::{ExternalIdentity, LinkConflict, NewExternalIdentity, Unlinked},
};
use serde_json::Value;
use tokio::sync::OnceCell;

use self::client::{Discovery, ProviderClient, ProviderError, TokenAuth, discovery_url, oauth_error, parse_github_user};
use crate::{
    DomainEvent, ServiceContext, ServiceError, Services, ThrottleScope, ThrottleService,
    audit::{AuditAction, AuditEntry, AuditService},
};
#[cfg(test)]
use rbit_core::AccountName;

/// The `server_secrets` row whose key seals flow cookies.
const KEY_NAME: &str = "external-sign-in";
/// How long discovery documents and key sets are reused.
const CACHE_TTL: Duration = Duration::from_hours(1);
/// Sent to GitHub's REST API.
const GITHUB_ACCEPT: &str = "application/vnd.github+json";

/// A configured provider, for sign-in buttons and settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
}

/// A verified callback, ready for [`Services::complete_external_sign_in`].
#[derive(Debug, Clone)]
pub struct Callback {
    pub identity: VerifiedIdentity,
    /// Where to go afterwards.
    pub next: String,
    /// The account that started a link, if this was one.
    pub link_account: Option<AccountId>,
}

/// The result of an external sign-in.
#[derive(Debug)]
pub enum ExternalSignIn {
    /// Open a session.
    SignedIn(User),
    /// Continue at the second step, like a password sign-in.
    SecondFactorRequired(SessionToken),
    /// The identity is now linked to the signed-in account that asked for it.
    Linked,
    /// Ask for a username; the token identifies the pending registration (keep it in a
    /// short-lived cookie).
    Register(SessionToken),
}

/// The "choose a username" step, pre-filled from the provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationDraft {
    pub provider_id: String,
    pub provider_name: String,
    /// A valid username suggested from the provider's login or address (may be empty).
    pub username: String,
    pub email: String,
    /// The provider vouches for `email`: it is marked verified if kept.
    pub email_verified: bool,
    pub next: String,
}

/// An identity linked to an account, for settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedIdentity {
    pub provider_id: String,
    /// The configured name, or the id of a provider no longer configured.
    pub provider_name: String,
    /// The provider is still configured (otherwise the identity can't sign in).
    pub configured: bool,
    pub username: Option<String>,
    pub email: Option<String>,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

/// How an account can sign in, for Settings → Security.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInMethods {
    pub has_password: bool,
    pub identities: Vec<LinkedIdentity>,
    /// Configured providers the account has not linked yet.
    pub linkable: Vec<ProviderSummary>,
}

impl SignInMethods {
    /// Unlinking `provider_id` would leave the account without a way to sign in.
    pub fn is_last_method(&self, provider_id: &str) -> bool {
        is_last_method(
            self.has_password,
            self.identities.iter().map(|identity| (identity.provider_id.as_str(), identity.configured)),
            provider_id,
        )
    }
}

/// A configured provider and what was learned about it.
#[derive(Debug)]
struct Provider {
    config: ProviderConfig,
    discovery: tokio::sync::Mutex<Option<(Arc<Discovery>, Instant)>>,
    keys: tokio::sync::Mutex<Option<(Arc<JwkSet>, Instant)>>,
}

#[derive(Debug, Clone)]
struct PendingRegistration {
    identity: VerifiedIdentity,
    next: String,
}

#[derive(Debug)]
struct Inner {
    providers: Vec<Provider>,
    /// `None` only if the HTTP client could not be built (logged at start-up).
    client: Option<ProviderClient>,
    base_url: String,
    database: Database,
    audit: AuditService,
    throttle: ThrottleService,
    key: OnceCell<FlowKey>,
    pending: PendingRegistrations<PendingRegistration>,
    original_links: links::OriginalLinks,
}

#[derive(Debug, Clone)]
pub struct ExternalAuthService {
    inner: Arc<Inner>,
}

impl ExternalAuthService {
    /// Fails when `[auth]` is invalid (the server refuses to start).
    pub(crate) fn new(ctx: &ServiceContext, throttle: ThrottleService) -> Result<Self, ServiceError> {
        let auth = &ctx.config.auth;
        auth.validate().map_err(|problems| ServiceError::Invalid(format!("invalid [auth] configuration: {problems}")))?;
        let client = if auth.providers.is_empty() {
            None
        } else {
            match ProviderClient::new(
                auth.allow_private_addresses,
                Duration::from_secs(auth.timeout_seconds.max(1)),
                ctx.config.updates.report_version,
            ) {
                Ok(client) => Some(client),
                Err(error) => {
                    tracing::error!(%error, "cannot build the HTTP client for sign-in providers; external sign-in is unavailable");
                    None
                }
            }
        };
        let providers = auth
            .providers
            .iter()
            .map(|config| Provider {
                config: config.clone(),
                discovery: tokio::sync::Mutex::default(),
                keys: tokio::sync::Mutex::default(),
            })
            .collect();
        Ok(Self {
            inner: Arc::new(Inner {
                providers,
                client,
                base_url: ctx.config.server.base_url().to_owned(),
                database: ctx.database.clone(),
                audit: AuditService::from_store(ctx.database.audit()),
                throttle,
                key: OnceCell::new(),
                pending: PendingRegistrations::default(),
                original_links: links::OriginalLinks::new(ctx.kv.sessions().epoch_sidecar()),
            }),
        })
    }

    /// The configured providers, in configuration order.
    pub fn providers(&self) -> Vec<ProviderSummary> {
        self.inner.providers.iter().map(|provider| summary(&provider.config)).collect()
    }

    fn provider(&self, id: &str) -> Option<&Provider> {
        self.inner.providers.iter().find(|provider| provider.config.id == id)
    }

    /// `{public_url}/login/{id}/callback`, to register at the provider.
    pub fn redirect_uri(&self, provider_id: &str) -> String {
        format!("{}/login/{provider_id}/callback", self.inner.base_url)
    }

    async fn key(&self) -> Result<&FlowKey, ServiceError> {
        self.inner
            .key
            .get_or_try_init(|| async {
                let mut fresh = [0_u8; 32];
                getrandom::fill(&mut fresh).map_err(|error| ServiceError::internal(std::io::Error::other(error.to_string())))?;
                let stored = self.inner.database.server_secrets().get_or_insert(KEY_NAME, &fresh).await?;
                FlowKey::new(&stored).ok_or_else(|| ServiceError::internal(std::io::Error::other("the stored sign-in key is too short")))
            })
            .await
    }

    fn client(&self) -> Result<&ProviderClient, ServiceError> {
        self.inner.client.as_ref().ok_or_else(|| ServiceError::Invalid("external sign-in is unavailable; see the server log".to_owned()))
    }

    /// The token that lets the signed-in session `session` start a link (put it in the
    /// "Link" URL; see [`FlowKey::link_token`]).
    pub async fn link_token(&self, session: &SessionToken) -> Result<String, ServiceError> {
        Ok(self.key().await?.link_token(&session.digest()))
    }

    /// Whether `token` is the link token of `session`.
    pub async fn check_link_token(&self, session: &SessionToken, token: &str) -> Result<bool, ServiceError> {
        Ok(self.key().await?.check_link_token(&session.digest(), token))
    }

    // ------------------------------------------------------------------ the flow

    /// Starts signing in (or, with `link_account`, linking) with `provider_id`. Returns
    /// the provider's authorization URL and the sealed flow for the cookie.
    pub async fn begin(&self, provider_id: &str, next: &str, link_account: Option<AccountId>) -> Result<(String, String), ServiceError> {
        let provider = self.provider(provider_id).ok_or(ServiceError::NotFound)?;
        let key = self.key().await?;
        let flow = Flow::new(provider_id, next, link_account.map(|account| account.0), Timestamp::now().seconds());
        self.authorization_flow(provider, key, &flow).await
    }

    async fn begin_flow(&self, flow: &Flow) -> Result<(String, String), ServiceError> {
        let provider = self.provider(&flow.provider).ok_or(ServiceError::NotFound)?;
        self.authorization_flow(provider, self.key().await?, flow).await
    }

    async fn authorization_flow(&self, provider: &Provider, key: &FlowKey, flow: &Flow) -> Result<(String, String), ServiceError> {
        let provider_id = &flow.provider;
        let pkce = key.pkce(flow);
        let config = &provider.config;
        let redirect_uri = self.redirect_uri(provider_id);
        let scopes = config.effective_scopes().join(" ");
        let challenge = pkce.challenge();
        let (endpoint, nonce) = match config.kind {
            ProviderKind::Oidc => (self.discovery(provider).await?.authorization_endpoint.clone(), Some(key.nonce(flow))),
            ProviderKind::Github => (format!("{}/login/oauth/authorize", config.github_base_url()), None),
        };
        let mut url = self.client()?.endpoint(&endpoint).map_err(|error| unreachable_provider(config, &error))?;
        url.query_pairs_mut().extend_pairs(provider::authorization_query(
            &config.client_id,
            &redirect_uri,
            &scopes,
            &flow.state,
            &challenge,
            nonce.as_deref(),
        ));
        Ok((url.into(), key.seal(flow)))
    }

    /// Checks a callback against the flow cookie and asks the provider who signed in.
    /// Throttled per client address; every refusal is audited and costs extra. Messages
    /// of [`ServiceError::Invalid`] are safe to show.
    pub async fn callback(
        &self,
        provider_id: &str,
        cookie: Option<&str>,
        params: CallbackParams<'_>,
        client: &ClientInfo,
    ) -> Result<Callback, ServiceError> {
        self.callback_with::<callback_mode::Legacy>(provider_id, cookie, params, client).await
    }

    /// One verified original callback: login retains its trusted Callback DTO, while
    /// manual linking requires the opaque context captured before provider redirect.
    pub async fn callback_original(
        &self,
        provider_id: &str,
        cookie: Option<&str>,
        params: CallbackParams<'_>,
        client: &ClientInfo,
    ) -> Result<OriginalProviderCallback, ServiceError> {
        self.callback_with::<callback_mode::Original>(provider_id, cookie, params, client).await
    }

    async fn callback_with<M: callback_mode::Mode>(
        &self,
        provider_id: &str,
        cookie: Option<&str>,
        params: CallbackParams<'_>,
        client: &ClientInfo,
    ) -> Result<M::Output, ServiceError> {
        let provider = self.provider(provider_id).ok_or(ServiceError::NotFound)?;
        let ip = ip_key(client);
        self.inner.throttle.check_client(ThrottleScope::ExternalSignIn, &ip)?;
        match self.verify_callback::<M>(provider, cookie, params).await {
            Ok(callback) => Ok(callback),
            Err(callback_mode::CheckError::Unavailable) => Err(ServiceError::Unavailable { retry_after_seconds: 1 }),
            Err(callback_mode::CheckError::Remote(Refusal { log, user })) => {
                tracing::info!(provider = provider_id, reason = %log, "external sign-in refused");
                self.inner.throttle.record_client_failure(ThrottleScope::ExternalSignIn, &ip);
                let entry =
                    AuditEntry::new(AuditAction::SignInFailed).from_ip(client.ip).detail(format!("{}: {user}", provider.config.name));
                self.inner.audit.record(entry).await;
                Err(ServiceError::Invalid(user))
            }
        }
    }

    async fn verify_callback<M: callback_mode::Mode>(
        &self,
        provider: &Provider,
        cookie: Option<&str>,
        params: CallbackParams<'_>,
    ) -> Result<M::Output, callback_mode::CheckError> {
        let config = &provider.config;
        let policy = policy(config);
        let key = self.key().await.map_err(|error| Refusal::new(error.to_string(), callback::FAILED))?;
        let (flow, code) = callback::check(key, &policy, cookie, params, Timestamp::now().seconds())?;
        M::before_exchange(self, &flow)?;
        let identity = match config.kind {
            ProviderKind::Oidc => self.oidc_identity(provider, key, &flow, code).await,
            ProviderKind::Github => self.github_identity(provider, key, &flow, code).await,
        }
        .map_err(|log| callback::unconfirmed(&policy, log))?;
        M::finish(self, flow, identity)
    }

    /// The provider's discovery document (cached for an hour, refetched on failure).
    async fn discovery(&self, provider: &Provider) -> Result<Arc<Discovery>, ServiceError> {
        let config = &provider.config;
        let mut cached = provider.discovery.lock().await;
        if let Some((discovery, fetched)) = cached.as_ref()
            && fetched.elapsed() < CACHE_TTL
        {
            return Ok(Arc::clone(discovery));
        }
        let issuer = config.issuer_url().unwrap_or_default();
        let document =
            self.client()?.get_json(&discovery_url(issuer), None, &[]).await.map_err(|error| unreachable_provider(config, &error))?;
        let discovery = Arc::new(Discovery::parse(document, issuer).map_err(|problem| {
            tracing::error!(provider = %config.id, %problem, "unusable OpenID Connect discovery document");
            ServiceError::Invalid(format!("{} is misconfigured; ask an administrator to check the server log", config.name))
        })?);
        for endpoint in [&discovery.authorization_endpoint, &discovery.token_endpoint, &discovery.jwks_uri] {
            self.client()?.endpoint(endpoint).map_err(|error| unreachable_provider(config, &error))?;
        }
        *cached = Some((Arc::clone(&discovery), Instant::now()));
        Ok(discovery)
    }

    /// The provider's signing keys; `refresh` fetches them again (the token named a key
    /// the cached set lacks: the provider rotated its keys).
    async fn keys(&self, provider: &Provider, discovery: &Discovery, refresh: bool) -> Result<Arc<JwkSet>, String> {
        let mut cached = provider.keys.lock().await;
        if !refresh
            && let Some((keys, fetched)) = cached.as_ref()
            && fetched.elapsed() < CACHE_TTL
        {
            return Ok(Arc::clone(keys));
        }
        let client = self.client().map_err(|error| error.to_string())?;
        let document = client.get_json(&discovery.jwks_uri, None, &[]).await.map_err(|error| error.to_string())?;
        let keys = Arc::new(JwkSet::parse(document.to_string().as_bytes()).map_err(|error| error.to_string())?);
        *cached = Some((Arc::clone(&keys), Instant::now()));
        Ok(keys)
    }

    async fn oidc_identity(&self, provider: &Provider, key: &FlowKey, flow: &Flow, code: &str) -> Result<VerifiedIdentity, String> {
        let config = &provider.config;
        let discovery = self.discovery(provider).await.map_err(|error| error.to_string())?;
        let client = self.client().map_err(|error| error.to_string())?;
        let redirect_uri = self.redirect_uri(&config.id);
        let pkce = key.pkce(flow);
        let mut form =
            vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", &redirect_uri), ("code_verifier", pkce.verifier())];
        let basic = match discovery.token_auth {
            TokenAuth::Basic => Some((config.client_id.as_str(), config.client_secret.as_str())),
            TokenAuth::Post => {
                form.extend([("client_id", config.client_id.as_str()), ("client_secret", config.client_secret.as_str())]);
                None
            }
        };
        let tokens = client.post_form(&discovery.token_endpoint, &form, basic).await.map_err(|error| error.to_string())?;
        if let Some(error) = oauth_error(&tokens) {
            return Err(format!("token endpoint: {error}"));
        }
        let id_token = tokens.get("id_token").and_then(Value::as_str).ok_or("the token response has no id_token")?;
        let nonce = key.nonce(flow);
        let check =
            IdTokenCheck { issuer: &discovery.issuer, client_id: &config.client_id, nonce: &nonce, now: Timestamp::now().seconds() };
        let mut keys = self.keys(provider, &discovery, false).await?;
        let mut verified = oidc::verify_id_token(id_token, &keys, &check);
        if verified == Err(OidcError::UnknownKey) {
            // Only tokens from the token endpoint get here, so an attacker can't make us
            // refetch at will; a provider that rotated its keys is served at once.
            keys = self.keys(provider, &discovery, true).await?;
            verified = oidc::verify_id_token(id_token, &keys, &check);
        }
        let token = verified.map_err(|error| format!("ID token refused: {error}"))?;
        let mut identity = provider::oidc_identity(&config.id, token);
        if needs_userinfo(&identity)
            && let (Some(userinfo), Some(access_token)) =
                (discovery.userinfo_endpoint.as_deref(), tokens.get("access_token").and_then(Value::as_str))
        {
            match client.get_json(userinfo, Some(access_token), &[]).await {
                Ok(info) => {
                    if !apply_userinfo(&mut identity, &info) {
                        tracing::warn!(provider = %config.id, "userinfo answered for another subject; ignored");
                    }
                }
                Err(error) => tracing::info!(provider = %config.id, %error, "userinfo request failed; continuing without an address"),
            }
        }
        Ok(identity)
    }

    async fn github_identity(&self, provider: &Provider, key: &FlowKey, flow: &Flow, code: &str) -> Result<VerifiedIdentity, String> {
        let config = &provider.config;
        let client = self.client().map_err(|error| error.to_string())?;
        let redirect_uri = self.redirect_uri(&config.id);
        let pkce = key.pkce(flow);
        let form = [
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", pkce.verifier()),
        ];
        let token_url = format!("{}/login/oauth/access_token", config.github_base_url());
        let tokens = client.post_form(&token_url, &form, None).await.map_err(|error| error.to_string())?;
        // GitHub reports a bad code with 200 and an `error` field.
        if let Some(error) = oauth_error(&tokens) {
            return Err(format!("token endpoint: {error}"));
        }
        let access_token = tokens.get("access_token").and_then(Value::as_str).ok_or("the token response has no access_token")?;
        let api = config.github_api_url();
        let headers = [("Accept", GITHUB_ACCEPT), ("X-GitHub-Api-Version", "2022-11-28")];
        let user = client.get_json(&format!("{api}/user"), Some(access_token), &headers).await.map_err(|error| error.to_string())?;
        let emails = match client.get_json(&format!("{api}/user/emails"), Some(access_token), &headers).await {
            Ok(emails) => Some(emails),
            Err(error) => {
                tracing::info!(provider = %config.id, %error, "GitHub did not list the user's addresses; continuing without one");
                None
            }
        };
        let user = parse_github_user(&user, emails.as_ref())?;
        Ok(provider::github_identity(&config.id, user.id, user.login, user.name, user.verified_email))
    }

    // ------------------------------------------------------------------ registration

    fn start_registration(&self, identity: VerifiedIdentity, next: String) -> SessionToken {
        self.inner.pending.start(PendingRegistration { identity, next })
    }

    fn pending_registration(&self, token: &SessionToken) -> Option<PendingRegistration> {
        self.inner.pending.get(token)
    }

    /// The "choose a username" step behind `token`, if it is still open and for
    /// `provider_id`.
    pub fn registration_draft(&self, provider_id: &str, token: &SessionToken) -> Option<RegistrationDraft> {
        let pending = self.pending_registration(token).filter(|pending| pending.identity.provider_id == provider_id)?;
        let provider_name = self.provider(provider_id)?.config.name.clone();
        let identity = &pending.identity;
        Some(RegistrationDraft {
            provider_id: provider_id.to_owned(),
            provider_name,
            username: suggested_username(identity),
            email: identity.email.clone().unwrap_or_default(),
            email_verified: vouched_email(identity),
            next: pending.next,
        })
    }

    /// Abandons a pending registration.
    pub fn end_registration(&self, token: &SessionToken) {
        self.inner.pending.end(token);
    }

    // ------------------------------------------------------------------ settings

    /// How `user` can sign in: password, linked identities, providers left to link.
    pub async fn methods(&self, user: &User) -> Result<SignInMethods, ServiceError> {
        let store = self.inner.database.external_identities();
        let has_password = store.has_password(user.id()).await?;
        let linked = store.list_for_account(user.id()).await?;
        let identities: Vec<LinkedIdentity> = linked
            .iter()
            .map(|identity| {
                let provider = self.provider(&identity.provider_id);
                LinkedIdentity {
                    provider_id: identity.provider_id.clone(),
                    provider_name: provider.map_or_else(|| identity.provider_id.clone(), |provider| provider.config.name.clone()),
                    configured: provider.is_some(),
                    username: identity.username.clone(),
                    email: identity.email.clone(),
                    created_at: identity.created_at,
                    last_used_at: identity.last_used_at,
                }
            })
            .collect();
        let linkable = self
            .inner
            .providers
            .iter()
            .filter(|provider| !linked.iter().any(|identity| identity.provider_id == provider.config.id))
            .map(|provider| summary(&provider.config))
            .collect();
        Ok(SignInMethods { has_password, identities, linkable })
    }

    /// Unlinks `user`'s identity at `provider_id`, unless it is the account's last way
    /// to sign in. Audited.
    pub async fn unlink(&self, user: &User, provider_id: &str, client: &ClientInfo) -> Result<(), ServiceError> {
        let usable: Vec<&str> = self.inner.providers.iter().map(|provider| provider.config.id.as_str()).collect();
        let store = self.inner.database.external_identities();
        match store.unlink(user.id(), provider_id, &usable).await? {
            Unlinked::Removed => {
                let name = self.provider(provider_id).map_or(provider_id, |provider| provider.config.name.as_str());
                let entry = AuditEntry::new(AuditAction::ExternalIdentityUnlinked).by(user).from_ip(client.ip).detail(name);
                self.inner.audit.record(entry).await;
                Ok(())
            }
            Unlinked::NotFound => Err(ServiceError::NotFound),
            Unlinked::LastMethod => Err(ServiceError::Invalid(LAST_METHOD.to_owned())),
        }
    }

    async fn record(&self, entry: AuditEntry) {
        self.inner.audit.record(entry).await;
    }
}

/// A configured provider's sign-in policy, for `rbit_identity::external`.
fn policy(config: &ProviderConfig) -> ProviderPolicy<'_> {
    let registration = match config.allow_registration {
        ProviderRegistration::Never => rbit_identity::external::ProviderRegistration::Never,
        ProviderRegistration::FollowSite => rbit_identity::external::ProviderRegistration::FollowSite,
        ProviderRegistration::Always => rbit_identity::external::ProviderRegistration::Always,
    };
    ProviderPolicy { id: &config.id, name: &config.name, trust_email: config.trust_email, trust_mfa: config.trust_mfa, registration }
}

fn summary(config: &ProviderConfig) -> ProviderSummary {
    ProviderSummary { id: config.id.clone(), name: config.name.clone(), kind: config.kind }
}

fn unreachable_provider(config: &ProviderConfig, error: &ProviderError) -> ServiceError {
    tracing::warn!(provider = %config.id, %error, "sign-in provider unavailable");
    ServiceError::Invalid(format!("{} can't be reached right now; try again in a moment", config.name))
}

/// The throttle key for a client address.
fn ip_key(client: &ClientInfo) -> String {
    client.ip.map_or_else(|| "unknown".to_owned(), |ip| ip.to_canonical().to_string())
}

// ---------------------------------------------------------------------- flows across services

mod flows;
mod original;
pub use original::OriginalExternalSignIn;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_suggested_from_the_provider() {
        assert_eq!(suggest_username(&[Some("octocat"), Some("other")]), "octocat");
        assert_eq!(suggest_username(&[Some("Jane Doe"), None]), "JaneDoe");
        assert_eq!(suggest_username(&[Some("settings"), Some("alice")]), "alice", "reserved names are skipped");
        assert_eq!(suggest_username(&[Some(".hidden")]), "hidden");
        assert_eq!(suggest_username(&[Some("!!!"), None]), "");
        assert_eq!(suggest_username(&[Some(&"a".repeat(80))]).len(), AccountName::MAX_LEN);
    }

    #[test]
    fn the_last_method_is_known() {
        let identity = |id: &str, configured: bool| LinkedIdentity {
            provider_id: id.to_owned(),
            provider_name: id.to_owned(),
            configured,
            username: None,
            email: None,
            created_at: Timestamp(1),
            last_used_at: None,
        };
        let mut methods = SignInMethods { has_password: false, identities: vec![identity("corp", true)], linkable: Vec::new() };
        assert!(methods.is_last_method("corp"));
        methods.identities.push(identity("old", false));
        assert!(methods.is_last_method("corp"), "an identity at a removed provider can't sign in");
        methods.identities.push(identity("github", true));
        assert!(!methods.is_last_method("corp"));
        methods.identities.truncate(1);
        methods.has_password = true;
        assert!(!methods.is_last_method("corp"));
    }
}
