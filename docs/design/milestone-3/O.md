# O — External sign-in: as built

Workstream O of [milestone 3](../milestone-3.md). Migration `0037` only.

## As built

- **Providers** (`[[auth.providers]]`, `rbit-core/src/config/auth.rs`): `kind = "oidc"` (any
  OpenID Connect provider, discovery at `{issuer}/.well-known/openid-configuration`) or
  `kind = "github"` (GitHub's OAuth web flow; `base_url` for GitHub Enterprise Server, API at
  `{base_url}/api/v3`). Fields: `id` (slug, used in URLs), `name`, `issuer`, `base_url`,
  `client_id`, `client_secret`, `scopes` (defaults `openid email profile` / `read:user
  user:email`; OIDC always gets `openid`), `allow_registration`, `trust_email`, `trust_mfa`.
  `[auth] timeout_seconds` (10) and `allow_private_addresses` (false; also allows `http`
  issuers). Everything is validated at start-up (`AuthConfig::validate`, called by the
  loader and again by `Services::new`) and every problem is reported at once.
- **Secrets from the environment.** figment cannot index into an array of tables (an
  indexed key replaces the whole array), so `rbit-server/src/config.rs` filters
  `RBIT_AUTH__PROVIDERS__*` out of figment and applies them with
  `AuthConfig::apply_provider_env`: `RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET` and
  `…__CLIENT_ID`, where `<N>` is the zero-based position **or** the provider's id
  (upper-cased, `-` as `_`). Other provider fields must be in the file. The secret never
  appears in `Debug` output.
- **Callback URL** to register at the provider: `{public_url}/login/{id}/callback`.
- **Flow** (`rbit-auth/src/oidc.rs`, `rbit-services/src/external_auth.rs`):
  authorization code + PKCE `S256`. A `Flow` (provider, random 256-bit `state`, `next`,
  linking account, issue time) is sealed with HMAC-SHA256 under a key kept in
  `server_secrets` (`external-sign-in`, generated on first use) into the `rbit_oauth`
  cookie (`HttpOnly`, `SameSite=Lax`, `Path=/login`, 10 minutes, removed by the callback).
  The PKCE verifier and the OIDC `nonce` are HMACs of the `state` under the same key, so
  nothing is stored server-side and the cookie holds no secret beyond the state. The
  returned `state` must match exactly (constant time).
- **OIDC verification**: discovery cached for an hour (issuer must equal the configured one,
  trailing slash aside; `S256` must not be ruled out; `client_secret_basic` unless only
  `client_secret_post` is offered); token exchange; ID token checked against the JWKS
  (RS256 with ≥ 2048-bit keys and ES256 only — `none`, `HS*` and the rest refused; key by
  `kid`; an unknown `kid` refetches the key set once, which is how rotation works), `iss`
  exactly, `aud` (with `azp` when there are several audiences or one is given), `exp`/`iat`/
  `nbf` with 60 s leeway, `nonce`, `sub`. Email from `email` + `email_verified` (bool or
  `"true"`); if the ID token has no address, the userinfo endpoint is asked (same `sub`
  required). `amr` containing `mfa` is recorded for `trust_mfa`.
- **GitHub**: `{base}/login/oauth/authorize` (with PKCE), `…/access_token` (a 200 with
  `error` is a failure), then `/user` (numeric `id` is the subject) and `/user/emails` —
  only the **verified primary** address is used; the public profile address never is.
- **Outbound HTTP**: one `reqwest` client per instance using the webhooks'
  `GuardedResolver` (internal addresses refused at connection time unless
  `allow_private_addresses`), no redirects, no proxy, the timeout on every request, 1 MiB
  response cap, `https` only unless private addresses are allowed, address literals
  checked before sending, `User-Agent: rbit/<version>`.
- **Linking rules** (`Services::complete_external_sign_in`), always by the provider's
  stable subject: (1) known identity → sign in as its account; (2) unknown and the flow is
  a link started from Settings → Security → link to that account (the callback must arrive
  in the same session); (3) unknown, `trust_email`, provider-verified address equal to an
  account's **verified** address → link and sign in; (4) unknown and registration allowed →
  "choose a username" (`/login/{id}/register`, pending state in memory for 15 minutes,
  `rbit_oauth_signup` cookie); (5) otherwise refuse with a page that explains how to link.
  An unverified address never links anything.
- **Registration rule** (`allow_registration`): `true` (default) follows the site's
  registration setting (`/admin/settings`, including "the first account can always be
  created"), `"always"` ignores it (the provider decides who gets in), `false` never
  registers. The address on the register step is pre-filled and marked verified only if it
  is the provider's verified one (editing it makes it unverified and a verification email
  goes out as for any sign-up; accounts that start verified get none). The first account of
  an instance becomes its administrator, however it was created.
- **Passwordless accounts.** Accounts registered through a provider store an empty password
  hash, which never verifies. A password reset link sets one (tested end to end). Settings
  → Security says so and that Git over HTTPS needs a personal access token.
- **2FA**: an external sign-in passes the password step only; accounts with 2FA continue at
  the existing `/login/two-factor` (the pending sign-in now records how its first step was
  passed, so the audit log says "GitHub and authenticator code"). With `trust_mfa = true`
  (OIDC only) and `mfa` in `amr`, the second step is skipped (audited as
  "Corp SSO (provider MFA)").
- **Suspended accounts** are refused (403 page) and the attempt is audited.
- **Settings → Security** ("Sign-in methods" section, shown when providers are configured
  or identities exist): password badge, linked identities (provider, username · email,
  linked / last used, "No longer available" for removed providers) with Unlink, or "Only
  way to sign in" when it is the last method (no password, no other identity at a
  configured provider — enforced atomically in the store as well), and Link buttons.
- **Starting is a link, not a form.** CSP `form-action 'self'` also applies to the redirect
  a form post leads to, so a POST would be blocked on its way to the provider. Sign-in
  start (`GET /login/{id}`) is harmless to trigger; link start
  (`GET /settings/security/linked/{id}?token=…`) needs a token that is an HMAC of the
  session digest, so another site cannot link an identity to a signed-in user. The callback
  is a `GET` protected by the state check. `/login/` was added to the hover-preload deny
  lists (`middleware.rs`, `nav.js`); the buttons carry `hx-boost="false"`.
- **Throttling**: `ThrottleScope::ExternalSignIn` per client address on every callback
  (burst 10×5, failures cost 3); the register step spends the sign-up budget.
- **Audit**: `SignIn` (detail: provider name), `SignInFailed` (provider: reason, also "no
  linked account" and "account suspended"), new `ExternalIdentityLinked` /
  `ExternalIdentityUnlinked` (category "Sign-in").

### Files

- core: `config/auth.rs` (new), `config/mod.rs` (`auth` field at the end of `Config`).
- server: `config.rs` (env filter + `apply_provider_env` + `validate`, with a test).
- auth: `oidc.rs` (new: `FlowKey`, `Flow`, `Pkce`, `JwkSet`, `verify_id_token`), `lib.rs`.
- storage: `migrations/0037_external_identities.sql`, `sqlite/external_identities.rs`,
  `Database::external_identities()`, `sqlite/audit.rs` (two actions appended),
  `tests/external_identities.rs`, `tests/upgrade.rs` (`external_identity_table_starts_empty`).
- services: `external_auth.rs`, `external_auth/client.rs` (new), `lib.rs` (field +
  construction), `throttle.rs` (`ExternalSignIn`), `two_factor.rs` (`start_pending` takes the
  first-step method, `pub(crate)`), `recovery.rs` (no verification email for an address that
  is already verified), `Cargo.toml` (`serde`), `tests/external_auth.rs`.
- web: `routes/external_auth.rs`, `views/external_auth.rs`, `templates/auth/external_register.html`,
  `templates/auth/external_refused.html`, `templates/auth/login.html` (buttons),
  `templates/settings/security.html` (section), `views/auth.rs` (`providers`),
  `routes/auth.rs` (`second_factor_redirect` shared), `views/security.rs` (`sign_in` field,
  two notices), `routes/security.rs` (`overview_model`/`no_store` `pub(super)`), `routes/mod.rs`,
  `views/mod.rs`, `middleware.rs`, `assets/nav.js`.
- e2e: `tests/e2e/external_auth.rs` (fake OIDC provider and fake GitHub), `Cargo.toml`
  (`p256`, `base64` dev-dependencies — already in the tree).

No new crates: RSA/ECDSA verification uses the `rsa` and `p256` crates passkeys already use;
`cargo deny` was therefore not needed.

## Deviations

- **The flow cookie is signed, not encrypted.** It holds only the state, `next`, the
  linking account id and a timestamp; the PKCE verifier and nonce are derived from the
  state server-side and never leave the server, so there is nothing to hide from the
  browser that owns the flow. Integrity is what matters (a forged linking account id).
- **One flow per browser at a time**: a second sign-in started in another tab replaces the
  cookie, and the first tab's callback is refused ("try again").
- **Key refetch on unknown `kid` is not rate limited**: only an ID token returned by the
  provider's own token endpoint (after a valid code exchange) can trigger it.
- **No "set a password" form for passwordless accounts** in settings: the existing reset
  link sets one (it proves the mailbox, like GitHub). Without a mail transport such an
  account can only get a password from an administrator (`/admin` → set password).
- **Account settings** (change email / change password) still ask for the current password,
  which a passwordless account doesn't have; they need a reset link first. Removing a 2FA
  factor likewise needs a password. Documented in the security section text.
- **Display name** from the provider is not copied into the profile (profile validation is
  another workstream's; the user can set it).
- **Pending registrations live in memory** (like pending 2FA sign-ins): a restart in the
  15-minute window means signing in again.
- **"Sign up with …" buttons** were not added to `/signup`; signing in with a provider
  registers when allowed, and the sign-in page offers it.

## CHANGELOG entry

```
### Added
- Sign in with OpenID Connect providers (Keycloak, Authentik, Google, Entra ID, …) and
  GitHub (github.com or Enterprise Server), configured as `[[auth.providers]]`.
  Authorization code flow with PKCE, signed state, nonce and full ID token validation
  (RS256/ES256, key rotation). First sign-ins register an account when allowed
  (`allow_registration`), or link to an existing one from Settings → Security; linking by
  email only with `trust_email` and verified addresses on both sides. Two-factor
  authentication still applies unless `trust_mfa` and the provider reports MFA. Linked
  accounts are listed with unlink; the last way to sign in can't be removed.
- `[auth]` configuration section (`timeout_seconds`, `allow_private_addresses`,
  `[[auth.providers]]`); client secrets also from `RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET`.
```

## README

Feature line: `- **External sign-in** — OpenID Connect and GitHub, with PKCE and verified ID
tokens; link providers to existing accounts or let them register new ones.`

Configuration table row:

| `[auth]` | `timeout_seconds` (10), `allow_private_addresses` (false), `[[auth.providers]]` (`kind`, `id`, `name`, `issuer` / `base_url`, `client_id`, `client_secret`, `scopes`, `allow_registration`, `trust_email`, `trust_mfa`) | Sign in with OpenID Connect providers and GitHub; callback `{public_url}/login/{id}/callback` |

## rbit.example.toml

```toml
# External sign-in. Register {public_url}/login/{id}/callback as the redirect URI at the
# provider. Secrets can come from RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET (N = position,
# from 0, or the provider id in upper case with '-' as '_').
[auth]
timeout_seconds = 10
# Allow providers on loopback/private addresses and plain-http issuers (on-premises IdPs).
allow_private_addresses = false

# [[auth.providers]]
# kind = "oidc"
# id = "sso"                       # lower-case slug, used in URLs; don't change it later
# name = "Company SSO"             # "Sign in with Company SSO"
# issuer = "https://sso.example.com/realms/main"
# client_id = "rbit"
# client_secret = "change-me"
# scopes = ["openid", "email", "profile"]
# # true: when the site's registration is open; "always": even when closed; false: never.
# allow_registration = true
# # Link a first sign-in to the account with the same verified email (both sides verified).
# trust_email = false
# # Skip rbit's own 2FA when the ID token reports MFA in `amr`.
# trust_mfa = false

# [[auth.providers]]
# kind = "github"
# id = "github"
# name = "GitHub"
# client_id = "Iv1.0123456789abcdef"
# client_secret = "change-me"
# # base_url = "https://github.example.com"   # GitHub Enterprise Server
```

## ARCHITECTURE section text

> ### External sign-in (milestone 3)
>
> Providers are configured as `[[auth.providers]]` (`kind = "oidc"` or `"github"`) and
> validated at start-up. `ExternalAuthService` (`rbit-services/src/external_auth.rs`, HTTP in
> `external_auth/client.rs`) runs the authorization code flow with PKCE `S256`: a `Flow`
> (provider, random state, next, linking account) is sealed with HMAC-SHA256 under a
> `server_secrets` key into the `rbit_oauth` cookie (`HttpOnly`, `SameSite=Lax`,
> `Path=/login`, 10 min); the PKCE verifier and the nonce are HMACs of the state. The
> callback requires the exact state, exchanges the code, and verifies the ID token with
> `rbit_auth::oidc` (RS256/ES256 only, key by `kid` with one refetch on an unknown key,
> `iss`, `aud`/`azp`, `exp`/`iat`/`nbf` ± 60 s, `nonce`); GitHub uses `/user` and the
> verified primary address from `/user/emails`. Outbound requests use the webhooks'
> guarded resolver and `[auth] allow_private_addresses`.
>
> `Services::complete_external_sign_in` decides by the provider's subject: known identity →
> sign in (suspension refused, the account's 2FA continues at `/login/two-factor` unless
> `trust_mfa` and `amr` has `mfa`); a link from settings (same session) → link; `trust_email`
> with verified addresses on both sides → link; registration allowed (`allow_registration`:
> `true` follows the site setting, `"always"`, `false`) → `/login/{id}/register`; otherwise a
> refusal page. Registered accounts have no password (empty hash) until a reset link sets
> one. Unlinking refuses the last way to sign in, atomically in SQL.
>
> Schema (`0037`): `external_identities(id, account_id → accounts CASCADE, provider_id,
> subject, email, username, created_at, last_used_at, UNIQUE(provider_id, subject),
> UNIQUE(account_id, provider_id))`.
>
> Routes: `GET /login/{id}`, `GET /login/{id}/callback`, `GET|POST /login/{id}/register`,
> `GET /settings/security/linked/{id}?token=…` (link start, token bound to the session),
> `POST /settings/security/linked/{id}/unlink`.

Security model rows to add:

| External sign-in | Code flow with PKCE `S256`; state in a signed, short-lived `HttpOnly; SameSite=Lax` cookie, matched exactly; nonce and verifier derived server-side; ID tokens RS256/ES256 only with `iss`/`aud`/`azp`/`exp`/`iat`/`nbf`/`nonce` checked; identities keyed by the provider's subject; linking by email only with `trust_email` and verified addresses on both sides; link starts need a session-bound token; callbacks throttled per address; the last way to sign in cannot be unlinked; 2FA still applies by default. Provider requests use the outbound SSRF guard. |

Also: `server_secrets` now holds a second key (`external-sign-in`).

## Deployment notes (for docs/deployment.md)

- Register the redirect URI `{public_url}/login/{id}/callback` at the provider; `public_url`
  must be the address users reach (it is also in the authorization request).
- Keycloak/Authentik/Entra: a confidential client with the standard (authorization code)
  flow; PKCE `S256` is sent always. Google: an OAuth client of type "Web application".
- GitHub: an OAuth App (or a GitHub App's OAuth credentials) with the callback URL above;
  scopes `read:user user:email`.
- On-premises IdPs on private addresses or plain http need
  `[auth] allow_private_addresses = true`.
- Changing a provider's `id` orphans its linked identities (they are stored under it).

## Integration requests

- `docs/ARCHITECTURE.md`, `README.md`, `CHANGELOG.md`, `deploy/rbit.example.toml`,
  `docs/deployment.md`: the texts above.
- `RecoveryService::handle` and other exhaustive matches: no new `DomainEvent` variants.
- `AuditAction::ALL` grew to 44 (`ExternalIdentityLinked`, `ExternalIdentityUnlinked`
  appended); if another workstream appends actions, merge both and fix the array length.
- `SecurityNotice::ALL` grew to 9 (`IdentityLinked`, `IdentityUnlinked`).
- `middleware::is_unsafe_to_preload` now lists `/login/` instead of `/login/two-factor`
  (which it covers); `nav.js` likewise.
- Optional follow-up (account settings, not O's file): for accounts without a password,
  the change-email and change-password forms could explain that a reset link sets the
  first password, using `ExternalAuthService::methods(user).has_password`.
- W (account-rename redirects) is unaffected: identities reference account ids.
