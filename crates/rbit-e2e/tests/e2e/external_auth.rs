//! External sign-in (milestone 3, O) end to end, against providers running in the test:
//!
//! * a fake OIDC provider: discovery, a key set holding a P-256 key generated for the
//!   test (rotated on demand), an authorize endpoint that signs the configured user in
//!   at once, and a token endpoint that checks the client's credentials, the redirect
//!   URI, the PKCE verifier and single use of codes before it returns an ES256 ID token
//!   with the request's nonce;
//! * a fake GitHub: the OAuth web flow, `/api/v3/user` and `/api/v3/user/emails`.
//!
//! The browser hop to the provider and back is followed by hand, so every redirect is
//! asserted.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode as AxumStatus, header},
    response::{IntoResponse, Redirect, Response as AxumResponse},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{SigningKey, signature::Signer as _};
use rbit_core::config::{MailTransport, ProviderConfig, ProviderKind};
use rbit_e2e::{Client, MailLog, Response, StatusCode, TestServer, attribute_after};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const CLIENT_ID: &str = "rbit-client";
const CLIENT_SECRET: &str = "s3cret-value";
const NEW_PASSWORD: &str = "a brand new passphrase";

// ------------------------------------------------------------------ the fake providers

/// Who the fake provider signs in next.
#[derive(Debug, Clone)]
struct FakeUser {
    subject: String,
    email: Option<String>,
    verified: bool,
    username: String,
}

impl FakeUser {
    fn new(subject: &str, email: &str, username: &str) -> Self {
        Self { subject: subject.to_owned(), email: Some(email.to_owned()), verified: true, username: username.to_owned() }
    }
}

/// An authorization code waiting for the token request.
#[derive(Debug, Clone)]
struct Grant {
    redirect_uri: String,
    challenge: String,
    nonce: Option<String>,
    user: FakeUser,
}

#[derive(Debug)]
struct FakeState {
    url: String,
    key: SigningKey,
    kid: String,
    user: FakeUser,
    codes: HashMap<String, Grant>,
    /// Access tokens issued (GitHub), and whose they are.
    tokens: HashMap<String, FakeUser>,
    counter: u64,
}

#[derive(Debug, Clone)]
struct Fake(Arc<Mutex<FakeState>>);

/// A fresh P-256 key, from the clock and a counter (good enough for a test).
fn generate_key(seed: u64) -> SigningKey {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut round = 0_u32;
    loop {
        let digest = Sha256::digest(format!("{nanos}/{seed}/{round}"));
        if let Ok(key) = SigningKey::from_slice(&digest) {
            return key;
        }
        round += 1;
    }
}

impl Fake {
    async fn start(routes: Router<Self>) -> Self {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = FakeState {
            url,
            key: generate_key(0),
            kid: "key-1".to_owned(),
            user: FakeUser::new("nobody", "nobody@example.test", "nobody"),
            codes: HashMap::new(),
            tokens: HashMap::new(),
            counter: 0,
        };
        let fake = Self(Arc::new(Mutex::new(state)));
        let app = routes.with_state(fake.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        fake
    }

    async fn oidc() -> Self {
        Self::start(
            Router::new()
                .route("/.well-known/openid-configuration", get(discovery))
                .route("/jwks", get(jwks))
                .route("/authorize", get(authorize))
                .route("/token", post(oidc_token)),
        )
        .await
    }

    async fn github() -> Self {
        Self::start(
            Router::new()
                .route("/login/oauth/authorize", get(authorize))
                .route("/login/oauth/access_token", post(github_token))
                .route("/api/v3/user", get(github_user))
                .route("/api/v3/user/emails", get(github_emails)),
        )
        .await
    }

    fn url(&self) -> String {
        self.0.lock().unwrap().url.clone()
    }

    fn sign_in_as(&self, user: FakeUser) {
        self.0.lock().unwrap().user = user;
    }

    /// Replaces the signing key; the key set then lists only the new one.
    fn rotate_key(&self) {
        let mut state = self.0.lock().unwrap();
        state.counter += 1;
        state.key = generate_key(state.counter + 1000);
        state.kid = format!("key-{}", state.counter + 1);
    }

    fn next_secret(&self, prefix: &str) -> String {
        let mut state = self.0.lock().unwrap();
        state.counter += 1;
        format!("{prefix}-{}-{}", state.counter, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())
    }
}

async fn discovery(State(fake): State<Fake>) -> Json<Value> {
    let url = fake.url();
    Json(json!({
        "issuer": url,
        "authorization_endpoint": format!("{url}/authorize"),
        "token_endpoint": format!("{url}/token"),
        "jwks_uri": format!("{url}/jwks"),
        "response_types_supported": ["code"],
        "id_token_signing_alg_values_supported": ["ES256"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"],
    }))
}

async fn jwks(State(fake): State<Fake>) -> Json<Value> {
    let state = fake.0.lock().unwrap();
    let point = state.key.verifying_key().to_sec1_point(false);
    let bytes = point.as_bytes();
    Json(json!({ "keys": [{
        "kty": "EC", "crv": "P-256", "use": "sig", "alg": "ES256", "kid": state.kid,
        "x": URL_SAFE_NO_PAD.encode(&bytes[1..33]), "y": URL_SAFE_NO_PAD.encode(&bytes[33..65]),
    }]}))
}

/// Signs the configured user in at once and sends the browser back with a code.
async fn authorize(State(fake): State<Fake>, Query(query): Query<HashMap<String, String>>) -> AxumResponse {
    assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
    assert_eq!(query.get("code_challenge_method").map(String::as_str), Some("S256"));
    let redirect_uri = query["redirect_uri"].clone();
    assert!(redirect_uri.ends_with("/callback"), "{redirect_uri}");
    let code = fake.next_secret("code");
    let grant = {
        let state = fake.0.lock().unwrap();
        Grant {
            redirect_uri: redirect_uri.clone(),
            challenge: query["code_challenge"].clone(),
            nonce: query.get("nonce").cloned(),
            user: state.user.clone(),
        }
    };
    fake.0.lock().unwrap().codes.insert(code.clone(), grant);
    let back = format!("{redirect_uri}?code={code}&state={}", query["state"]);
    Redirect::to(&back).into_response()
}

/// Takes (single use) the grant behind a token request, if every check passes.
fn redeem(fake: &Fake, form: &HashMap<String, String>) -> Option<Grant> {
    let grant = fake.0.lock().unwrap().codes.remove(form.get("code")?)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(form.get("code_verifier")?.as_bytes()));
    (form.get("redirect_uri") == Some(&grant.redirect_uri) && challenge == grant.challenge).then_some(grant)
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let encoded = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Basic ")?;
    let decoded = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(encoded).ok()?).ok()?;
    let (id, secret) = decoded.split_once(':')?;
    Some((id.to_owned(), secret.to_owned()))
}

async fn oidc_token(State(fake): State<Fake>, headers: HeaderMap, Form(form): Form<HashMap<String, String>>) -> AxumResponse {
    if basic_credentials(&headers) != Some((CLIENT_ID.to_owned(), CLIENT_SECRET.to_owned())) {
        return (AxumStatus::UNAUTHORIZED, Json(json!({ "error": "invalid_client" }))).into_response();
    }
    assert_eq!(form.get("grant_type").map(String::as_str), Some("authorization_code"));
    let Some(grant) = redeem(&fake, &form) else {
        return (AxumStatus::BAD_REQUEST, Json(json!({ "error": "invalid_grant" }))).into_response();
    };
    let state = fake.0.lock().unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let mut claims = json!({
        "iss": state.url, "aud": CLIENT_ID, "sub": grant.user.subject, "iat": now, "exp": now + 300,
        "nonce": grant.nonce, "email_verified": grant.user.verified, "preferred_username": grant.user.username,
    });
    if let Some(email) = &grant.user.email {
        claims["email"] = json!(email);
    }
    let header = json!({ "alg": "ES256", "typ": "JWT", "kid": state.kid });
    let signed = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature: p256::ecdsa::Signature = state.key.sign(signed.as_bytes());
    let id_token = format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));
    Json(json!({ "access_token": "opaque", "token_type": "Bearer", "expires_in": 300, "id_token": id_token })).into_response()
}

async fn github_token(State(fake): State<Fake>, Form(form): Form<HashMap<String, String>>) -> Json<Value> {
    if form.get("client_id").map(String::as_str) != Some(CLIENT_ID) || form.get("client_secret").map(String::as_str) != Some(CLIENT_SECRET)
    {
        return Json(json!({ "error": "incorrect_client_credentials" }));
    }
    // Like GitHub: a bad code is a 200 with an error.
    let Some(grant) = redeem(&fake, &form) else {
        return Json(json!({ "error": "bad_verification_code", "error_description": "The code passed is incorrect or expired." }));
    };
    let token = fake.next_secret("gho");
    fake.0.lock().unwrap().tokens.insert(token.clone(), grant.user);
    Json(json!({ "access_token": token, "token_type": "bearer", "scope": "read:user,user:email" }))
}

fn github_caller(fake: &Fake, headers: &HeaderMap) -> Option<FakeUser> {
    let token = headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")?;
    assert!(headers.get(header::USER_AGENT).is_some(), "GitHub requires a User-Agent");
    fake.0.lock().unwrap().tokens.get(token).cloned()
}

async fn github_user(State(fake): State<Fake>, headers: HeaderMap) -> AxumResponse {
    let Some(user) = github_caller(&fake, &headers) else { return AxumStatus::UNAUTHORIZED.into_response() };
    Json(json!({ "id": user.subject.parse::<u64>().unwrap(), "login": user.username, "name": "The Octocat", "email": null }))
        .into_response()
}

async fn github_emails(State(fake): State<Fake>, headers: HeaderMap) -> AxumResponse {
    let Some(user) = github_caller(&fake, &headers) else { return AxumStatus::UNAUTHORIZED.into_response() };
    let emails: Vec<Value> = user
        .email
        .iter()
        .map(|email| json!({ "email": email, "primary": true, "verified": user.verified, "visibility": "private" }))
        .chain(std::iter::once(json!({ "email": "old@example.test", "primary": false, "verified": true })))
        .collect();
    Json(Value::Array(emails)).into_response()
}

// ------------------------------------------------------------------ helpers

fn provider(kind: ProviderKind, id: &str, name: &str, fake: &Fake) -> ProviderConfig {
    ProviderConfig {
        kind,
        id: id.to_owned(),
        name: name.to_owned(),
        issuer: (kind == ProviderKind::Oidc).then(|| fake.url()),
        base_url: (kind == ProviderKind::Github).then(|| fake.url()),
        client_id: CLIENT_ID.to_owned(),
        client_secret: CLIENT_SECRET.to_owned(),
        ..ProviderConfig::default()
    }
}

fn server_with(providers: Vec<ProviderConfig>) -> TestServer {
    TestServer::start_with(move |config| {
        config.auth.allow_private_addresses = true;
        config.auth.providers = providers;
        config.mail.transport = MailTransport::Log;
        "rbit@example.test".clone_into(&mut config.mail.from_address);
    })
}

/// Starts at `start` (a path on rbit), follows the redirect to the provider (which
/// signs its configured user in at once) and back, and returns rbit's answer to the
/// callback.
async fn through_provider(client: &Client, server: &TestServer, start: &str) -> Response {
    let started = client.get(start).await;
    started.expect_status(StatusCode::SEE_OTHER);
    assert_eq!(started.header("cache-control"), Some("no-store"));
    let set_cookie = started.header("set-cookie").expect("the flow cookie");
    assert!(
        set_cookie.starts_with("rbit_oauth=") && set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"),
        "{set_cookie}"
    );
    let authorize = started.location().expect("a redirect to the provider").to_owned();
    assert!(authorize.contains("code_challenge=") && authorize.contains("state="), "{authorize}");
    let browser = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).no_proxy().build().unwrap();
    let answer = browser.get(&authorize).send().await.unwrap();
    assert_eq!(answer.status(), StatusCode::SEE_OTHER);
    let back = answer.headers()[reqwest::header::LOCATION].to_str().unwrap().to_owned();
    let path = back.strip_prefix(server.base_url()).expect("the provider sends the browser back to rbit");
    client.get(path).await
}

// ------------------------------------------------------------------ scenarios

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oidc_register_sign_in_link_and_unlink() {
    let mail = MailLog::capture();
    let fake = Fake::oidc().await;
    let server = server_with(vec![provider(ProviderKind::Oidc, "corp", "Corp SSO", &fake)]);
    server.sign_up("root").await;

    // A first sign-in registers, pre-filled from the provider.
    let carol = server.client();
    carol.get("/login").await.expect_status(StatusCode::OK).expect_contains(&["Sign in with Corp SSO", r#"href="/login/corp""#]);
    fake.sign_in_as(FakeUser::new("sub-carol", "carol@corp.example", "carol"));
    through_provider(&carol, &server, "/login/corp").await.expect_redirect("/login/corp/register");
    carol.get("/login/corp/register").await.expect_status(StatusCode::OK).expect_contains(&[
        r#"value="carol""#,
        r#"value="carol@corp.example""#,
        "Verified by Corp SSO",
        "personal access token",
    ]);
    carol
        .post("/login/corp/register", &[("username", "root"), ("email", "carol@corp.example")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    carol.post("/login/corp/register", &[("username", "carol"), ("email", "carol@corp.example")]).await.expect_redirect("/");
    carol.get("/settings/security").await.expect_status(StatusCode::OK).expect_contains(&[
        "Sign-in methods",
        "No password",
        "Only way to sign in",
        "carol · carol@corp.example",
        "Linked a sign-in provider",
    ]);
    let refused = carol.post("/settings/security/linked/corp/unlink", &[]).await;
    refused.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["This is the only way to sign in to your account"]);

    // Sign out, sign in again: straight in, to where the sign-in started.
    carol.sign_out().await;
    through_provider(&carol, &server, "/login/corp?next=%2Fsettings%2Fsecurity").await.expect_redirect("/settings/security");
    carol.get("/settings/security").await.expect_status(StatusCode::OK);

    // A password from a reset link makes the identity removable. The provider verified
    // the address, so the only email is the reset itself.
    carol.sign_out().await;
    carol.post("/password/forgot", &[("email", "carol@corp.example")]).await.expect_redirect("/password/forgot?sent=1");
    let sent = mail.wait_for(&server, "carol@corp.example", 1).await;
    let link = sent[0].link_starting_with(&server.url("/password/reset/")).expect("a reset link");
    let path = link.strip_prefix(server.base_url()).unwrap().to_owned();
    carol.post(&path, &[("new_password", NEW_PASSWORD), ("confirm_password", NEW_PASSWORD)]).await.expect_status(StatusCode::SEE_OTHER);
    let carol = server.sign_in("carol", NEW_PASSWORD).await;
    let page = carol.get("/settings/security").await;
    page.expect_contains(&[r#"<span class="badge">Password</span>"#, "Unlink Corp SSO"]);

    // Linking to an existing password account from settings.
    let alice = server.sign_up("alice").await;
    let page = alice.get("/settings/security").await;
    page.expect_contains(&["Corp SSO", "Not linked"]);
    let link = attribute_after(&page.body, r#"class="btn shrink-0" hx-boost="false""#, "href").expect("a link button");
    alice.get("/settings/security/linked/corp?token=forged").await.expect_redirect("/settings/security");
    let link = link.replace("&amp;", "&");
    fake.sign_in_as(FakeUser::new("sub-carol", "carol@corp.example", "carol"));
    through_provider(&alice, &server, &link)
        .await
        .expect_status(StatusCode::FORBIDDEN)
        .expect_contains(&["already linked to another account"]);
    fake.sign_in_as(FakeUser::new("sub-alice", "alice@corp.example", "alice.c"));
    through_provider(&alice, &server, &link).await.expect_redirect("/settings/security?saved=identity-linked");
    alice.get("/settings/security?saved=identity-linked").await.expect_contains(&["Account linked.", "Unlink Corp SSO", "alice.c"]);

    // The provider rotates its keys: the new kid makes rbit fetch the key set again.
    alice.sign_out().await;
    fake.rotate_key();
    through_provider(&alice, &server, "/login/corp").await.expect_redirect("/");
    alice.get("/settings/security").await.expect_status(StatusCode::OK).expect_contains(&["alice.c"]);

    // Unlink: the identity no longer signs in (it would register a new account).
    alice.post("/settings/security/linked/corp/unlink", &[]).await.expect_redirect("/settings/security?saved=identity-unlinked");
    alice.get("/settings/security?saved=identity-unlinked").await.expect_contains(&[
        "Account unlinked.",
        "Not linked",
        "Unlinked a sign-in provider",
    ]);
    alice.sign_out().await;
    through_provider(&alice, &server, "/login/corp").await.expect_redirect("/login/corp/register");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callbacks_need_the_browser_that_started_them() {
    let fake = Fake::oidc().await;
    let server = server_with(vec![provider(ProviderKind::Oidc, "corp", "Corp SSO", &fake)]);
    fake.sign_in_as(FakeUser::new("sub-x", "x@corp.example", "x"));

    // No flow cookie at all.
    let stranger = server.client();
    let forged = stranger.get("/login/corp/callback?code=x&state=y").await;
    forged.expect_status(StatusCode::BAD_REQUEST).expect_contains(&["took too long or was started in another browser"]);

    // A callback carried to another browser (login CSRF) fails there.
    let victim = server.client();
    let attacker = server.client();
    let started = attacker.get("/login/corp").await;
    let browser = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).no_proxy().build().unwrap();
    let answer = browser.get(started.location().unwrap()).send().await.unwrap();
    let back = answer.headers()[reqwest::header::LOCATION].to_str().unwrap().to_owned();
    let path = back.strip_prefix(server.base_url()).unwrap().to_owned();
    victim.get(&path).await.expect_status(StatusCode::BAD_REQUEST);
    // The attacker's own browser could use it, but codes work once: the victim's try
    // didn't spend it (the state check comes first), so this succeeds once...
    attacker.get(&path).await.expect_redirect("/login/corp/register");
    // ...and a replay of the same callback is refused (its flow cookie is gone).
    attacker.get(&path).await.expect_status(StatusCode::BAD_REQUEST);

    // The provider refusing access is explained.
    let started = server.client();
    let response = started.get("/login/corp").await;
    let state = response.location().unwrap().split("state=").nth(1).unwrap().split('&').next().unwrap().to_owned();
    started
        .get(&format!("/login/corp/callback?error=access_denied&state={state}"))
        .await
        .expect_status(StatusCode::BAD_REQUEST)
        .expect_contains(&["sign you in (access was denied)"]);
    server.client().get("/login/nope").await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn github_sign_in_uses_the_verified_primary_address() {
    let github = Fake::github().await;
    let server = server_with(vec![provider(ProviderKind::Github, "github", "GitHub", &github)]);
    server.sign_up("root").await;
    github.sign_in_as(FakeUser::new("583231", "octocat@example.test", "octocat"));

    let client = server.client();
    client.get("/login").await.expect_contains(&["Sign in with GitHub"]);
    through_provider(&client, &server, "/login/github").await.expect_redirect("/login/github/register");
    client.get("/login/github/register").await.expect_contains(&[
        r#"value="octocat""#,
        r#"value="octocat@example.test""#,
        "Verified by GitHub",
    ]);
    client.post("/login/github/register", &[("username", "octocat"), ("email", "octocat@example.test")]).await.expect_redirect("/");
    client.get("/settings/security").await.expect_contains(&["GitHub", "octocat · octocat@example.test"]);
    client.sign_out().await;
    through_provider(&client, &server, "/login/github").await.expect_redirect("/");

    // An unverified primary address is offered, but not as verified.
    github.sign_in_as(FakeUser { verified: false, ..FakeUser::new("9", "hubot@example.test", "hubot") });
    let other = server.client();
    through_provider(&other, &server, "/login/github").await.expect_redirect("/login/github/register");
    let page = other.get("/login/github/register").await;
    page.expect_contains(&[r#"value="hubot""#, r#"value="""#]);
    assert!(!page.contains("Verified by GitHub"));
}
