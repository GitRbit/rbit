//! `[auth]`: signing in with external identity providers (OIDC, GitHub).
//!
//! ```toml
//! [auth]
//! # Upper bound for each request to a provider (discovery, keys, token exchange, user API).
//! timeout_seconds = 10
//! # Providers on loopback or private addresses (an on-premises IdP, tests) are refused
//! # unless this is set; it also allows plain-http issuers.
//! allow_private_addresses = false
//!
//! [[auth.providers]]
//! kind = "oidc"                    # or "github"
//! id = "corp"                      # used in URLs: {public_url}/login/corp/callback
//! name = "Corp SSO"                # the button says "Sign in with Corp SSO"
//! issuer = "https://sso.example.com/realms/main"   # discovery at /.well-known/openid-configuration
//! client_id = "rbit"
//! client_secret = "…"              # or RBIT_AUTH__PROVIDERS__0__CLIENT_SECRET
//! scopes = ["openid", "email", "profile"]
//! allow_registration = true        # false: never; "always": even when sign-up is closed
//! trust_email = false              # link to the account with the same verified email
//! trust_mfa = false                # skip rbit's own 2FA when the IdP reports MFA (`amr`)
//! ```
//!
//! For `kind = "github"`, `issuer` is not used; `base_url` (default `https://github.com`)
//! points at a GitHub Enterprise Server instead, whose API is `{base_url}/api/v3`.
//!
//! Secrets from the environment: figment cannot index into an array of tables, so the
//! server loader keeps `RBIT_AUTH__PROVIDERS__<N>__CLIENT_ID` / `…__CLIENT_SECRET` (where
//! `<N>` is the provider's zero-based position or its `id`, upper-cased with `-` as `_`)
//! away from figment and applies them with [`AuthConfig::apply_provider_env`].
//!
//! Milestone 3 workstream O (external sign-in) owns this file.

use std::{collections::HashSet, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Longest provider id.
pub const MAX_PROVIDER_ID_LEN: usize = 32;
/// Provider ids that would collide with fixed routes under `/login/`.
const RESERVED_PROVIDER_IDS: [&str; 2] = ["two-factor", "callback"];
/// Environment keys (after `RBIT_`) that address a provider.
const PROVIDER_ENV_PREFIX: &str = "AUTH__PROVIDERS__";
/// Provider fields that may come from the environment.
const ENV_FIELDS: [&str; 2] = ["CLIENT_ID", "CLIENT_SECRET"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Upper bound for one request to a provider.
    pub timeout_seconds: u64,
    /// Allow providers whose addresses are loopback, private, link-local or unique-local,
    /// and issuers or base URLs over plain `http`.
    pub allow_private_addresses: bool,
    pub providers: Vec<ProviderConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self { timeout_seconds: 10, allow_private_addresses: false, providers: Vec::new() }
    }
}

/// Which protocol a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// Any OIDC provider (Keycloak, Authentik, Google, Entra ID, …).
    #[default]
    Oidc,
    /// GitHub's OAuth web flow (not OIDC), `github.com` or Enterprise Server.
    Github,
}

impl ProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oidc => "oidc",
            Self::Github => "github",
        }
    }
}

/// Whether a first sign-in with an unknown identity may create an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderRegistration {
    /// Only while the site's registration setting is open (`true` in the file).
    #[default]
    FollowSite,
    /// Never: identities must be linked to existing accounts (`false`).
    Never,
    /// Even when the site's sign-up form is closed (`"always"`): the provider decides
    /// who gets in.
    Always,
}

impl Serialize for ProviderRegistration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::FollowSite => serializer.serialize_bool(true),
            Self::Never => serializer.serialize_bool(false),
            Self::Always => serializer.serialize_str("always"),
        }
    }
}

impl<'de> Deserialize<'de> for ProviderRegistration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Text(String),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Flag(true) => Ok(Self::FollowSite),
            Raw::Flag(false) => Ok(Self::Never),
            // Environment variables arrive as text.
            Raw::Text(text) => match text.trim().to_ascii_lowercase().as_str() {
                "always" => Ok(Self::Always),
                "true" => Ok(Self::FollowSite),
                "false" => Ok(Self::Never),
                other => Err(serde::de::Error::custom(format!("allow_registration must be true, false or \"always\", not {other:?}"))),
            },
        }
    }
}

/// One `[[auth.providers]]` entry.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// Lower-case slug used in URLs (`/login/{id}`); never change it once people have
    /// linked identities, they are stored under it.
    pub id: String,
    /// Shown on buttons and in settings.
    pub name: String,
    /// OIDC issuer URL (`kind = "oidc"` only).
    pub issuer: Option<String>,
    /// GitHub Enterprise Server URL (`kind = "github"` only); default `https://github.com`.
    pub base_url: Option<String>,
    pub client_id: String,
    /// Never logged or shown (see the `Debug` implementation).
    pub client_secret: String,
    /// Requested scopes; the defaults are `openid email profile` (OIDC) and
    /// `read:user user:email` (GitHub). OIDC always gets `openid`.
    pub scopes: Option<Vec<String>>,
    pub allow_registration: ProviderRegistration,
    /// Link a first sign-in to the one account whose verified email equals the
    /// provider's verified email. Only for providers that control their users' addresses.
    pub trust_email: bool,
    /// Skip the account's own two-factor step when the ID token's `amr` claim says the
    /// provider performed multi-factor authentication (`"mfa"`). OIDC only.
    pub trust_mfa: bool,
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("kind", &self.kind)
            .field("id", &self.id)
            .field("name", &self.name)
            .field("issuer", &self.issuer)
            .field("base_url", &self.base_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &if self.client_secret.is_empty() { "" } else { "<redacted>" })
            .field("scopes", &self.scopes)
            .field("allow_registration", &self.allow_registration)
            .field("trust_email", &self.trust_email)
            .field("trust_mfa", &self.trust_mfa)
            .finish()
    }
}

impl ProviderConfig {
    /// The scopes to request.
    pub fn effective_scopes(&self) -> Vec<String> {
        let mut scopes = self.scopes.clone().unwrap_or_else(|| {
            let defaults: &[&str] = match self.kind {
                ProviderKind::Oidc => &["openid", "email", "profile"],
                ProviderKind::Github => &["read:user", "user:email"],
            };
            defaults.iter().map(|scope| (*scope).to_owned()).collect()
        });
        if self.kind == ProviderKind::Oidc && !scopes.iter().any(|scope| scope == "openid") {
            scopes.insert(0, "openid".to_owned());
        }
        scopes
    }

    /// The issuer without a trailing slash.
    pub fn issuer_url(&self) -> Option<&str> {
        self.issuer.as_deref().map(|issuer| issuer.trim().trim_end_matches('/'))
    }

    /// GitHub's web base (`https://github.com` or the Enterprise Server URL).
    pub fn github_base_url(&self) -> &str {
        self.base_url.as_deref().map_or("https://github.com", |url| url.trim().trim_end_matches('/'))
    }

    /// GitHub's REST API base: `https://api.github.com`, or `{base_url}/api/v3`.
    pub fn github_api_url(&self) -> String {
        match self.base_url.as_deref() {
            None => "https://api.github.com".to_owned(),
            Some(_) => format!("{}/api/v3", self.github_base_url()),
        }
    }
}

impl AuthConfig {
    /// Whether `key` (a figment environment key without the prefix, e.g.
    /// `auth__providers__0__client_secret`) addresses a provider field. The loader hides
    /// these from figment (an indexed key would replace the whole providers array) and
    /// applies them with [`Self::apply_provider_env`].
    pub fn is_provider_env_key(key: &str) -> bool {
        key.len() > PROVIDER_ENV_PREFIX.len()
            && key.get(..PROVIDER_ENV_PREFIX.len()).is_some_and(|start| start.eq_ignore_ascii_case(PROVIDER_ENV_PREFIX))
    }

    /// Applies `RBIT_AUTH__PROVIDERS__<N>__<FIELD>` variables, where `vars` yields the
    /// keys without the `RBIT_` prefix. `<N>` is a zero-based position or a provider id
    /// (upper-cased, `-` written as `_`); `<FIELD>` is `CLIENT_ID` or `CLIENT_SECRET`.
    pub fn apply_provider_env<I, K, V>(&mut self, vars: I) -> Result<(), String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        for (key, value) in vars {
            let key = key.as_ref();
            if !Self::is_provider_env_key(key) {
                continue;
            }
            let rest = &key[PROVIDER_ENV_PREFIX.len()..];
            let Some((which, field)) = rest.split_once("__") else {
                return Err(format!("RBIT_{key}: expected RBIT_AUTH__PROVIDERS__<N>__CLIENT_SECRET"));
            };
            let index = match which.parse::<usize>() {
                Ok(index) => Some(index),
                Err(_) => self.providers.iter().position(|provider| provider.id.replace('-', "_").eq_ignore_ascii_case(which)),
            };
            let Some(provider) = index.and_then(|index| self.providers.get_mut(index)) else {
                return Err(format!("RBIT_{key}: there is no [[auth.providers]] entry {which} (configure providers in the file)"));
            };
            let field = field.to_ascii_uppercase();
            match field.as_str() {
                "CLIENT_ID" => provider.client_id = value.into(),
                "CLIENT_SECRET" => provider.client_secret = value.into(),
                _ => return Err(format!("RBIT_{key}: only {} can be set from the environment", ENV_FIELDS.join(" and "))),
            }
        }
        Ok(())
    }

    /// Checks the providers, returning every problem found (empty when valid).
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut ids = HashSet::new();
        for (index, provider) in self.providers.iter().enumerate() {
            let label =
                if provider.id.is_empty() { format!("auth.providers[{index}]") } else { format!("auth provider {:?}", provider.id) };
            if let Err(problem) = validate_id(&provider.id) {
                problems.push(format!("{label}: {problem}"));
            } else if !ids.insert(provider.id.as_str()) {
                problems.push(format!("{label}: the id is used by another provider"));
            }
            if provider.name.trim().is_empty() {
                problems.push(format!("{label}: `name` is required (it is shown on the sign-in button)"));
            }
            if provider.client_id.trim().is_empty() {
                problems.push(format!("{label}: `client_id` is required"));
            }
            if provider.client_secret.is_empty() {
                problems.push(format!("{label}: `client_secret` is required (or set RBIT_AUTH__PROVIDERS__{index}__CLIENT_SECRET)"));
            }
            if provider
                .scopes
                .as_ref()
                .is_some_and(|scopes| scopes.iter().any(|scope| scope.is_empty() || scope.contains(char::is_whitespace)))
            {
                problems.push(format!("{label}: scopes must be single words"));
            }
            match provider.kind {
                ProviderKind::Oidc => {
                    match provider.issuer_url() {
                        None | Some("") => problems.push(format!("{label}: `issuer` is required for kind = \"oidc\"")),
                        Some(issuer) => {
                            if let Err(problem) = self.validate_url(issuer) {
                                problems.push(format!("{label}: issuer {problem}"));
                            }
                        }
                    }
                    if provider.base_url.is_some() {
                        problems.push(format!("{label}: `base_url` is for kind = \"github\"; OIDC providers use `issuer`"));
                    }
                }
                ProviderKind::Github => {
                    if provider.issuer.is_some() {
                        problems.push(format!("{label}: `issuer` is for kind = \"oidc\"; GitHub Enterprise uses `base_url`"));
                    }
                    if provider.trust_mfa {
                        problems.push(format!("{label}: `trust_mfa` needs an OIDC provider (GitHub does not report how users signed in)"));
                    }
                    if provider.base_url.is_some()
                        && let Err(problem) = self.validate_url(provider.github_base_url())
                    {
                        problems.push(format!("{label}: base_url {problem}"));
                    }
                }
            }
        }
        problems
    }

    /// Fails with every problem joined, for start-up.
    pub fn validate(&self) -> Result<(), String> {
        let problems = self.problems();
        if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) }
    }

    fn validate_url(&self, url: &str) -> Result<(), String> {
        let rest = if let Some(rest) = url.strip_prefix("https://") {
            rest
        } else if let Some(rest) = url.strip_prefix("http://") {
            if !self.allow_private_addresses {
                return Err("must use https (plain http needs [auth] allow_private_addresses = true)".to_owned());
            }
            rest
        } else {
            return Err("must be an absolute https URL".to_owned());
        };
        if rest.is_empty() || rest.starts_with('/') || url.chars().any(|c| c.is_whitespace() || c.is_control()) || url.contains(['?', '#'])
        {
            return Err("must be an absolute URL without query or fragment".to_owned());
        }
        Ok(())
    }
}

/// `[a-z0-9-]`, starting with a letter or digit, at most [`MAX_PROVIDER_ID_LEN`].
fn validate_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("`id` is required (it is used in URLs, e.g. \"github\")");
    }
    if id.len() > MAX_PROVIDER_ID_LEN {
        return Err("`id` can be at most 32 characters");
    }
    if !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') || id.starts_with('-') {
        return Err("`id` may only contain lower-case letters, digits and '-' (and not start with '-')");
    }
    if RESERVED_PROVIDER_IDS.contains(&id) {
        return Err("this `id` is reserved");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oidc(id: &str) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Oidc,
            id: id.to_owned(),
            name: "Corp".to_owned(),
            issuer: Some("https://sso.example.com/realms/main/".to_owned()),
            client_id: "rbit".to_owned(),
            client_secret: "s3cret".to_owned(),
            ..ProviderConfig::default()
        }
    }

    fn github() -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Github,
            id: "github".to_owned(),
            name: "GitHub".to_owned(),
            client_id: "Iv1.abc".to_owned(),
            client_secret: "s3cret".to_owned(),
            ..ProviderConfig::default()
        }
    }

    fn config(providers: Vec<ProviderConfig>) -> AuthConfig {
        AuthConfig { providers, ..AuthConfig::default() }
    }

    #[test]
    fn parses_providers_and_registration_modes() {
        let config: AuthConfig = serde_json::from_str(
            r#"{"providers": [
                {"kind": "oidc", "id": "corp", "name": "Corp", "issuer": "https://sso.example.com", "client_id": "a", "client_secret": "b", "trust_email": true},
                {"kind": "github", "id": "github", "name": "GitHub", "client_id": "c", "client_secret": "d", "allow_registration": "always"},
                {"kind": "github", "id": "ghe", "name": "GHE", "client_id": "c", "client_secret": "d", "allow_registration": false, "base_url": "https://ghe.example.com/"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(config.timeout_seconds, 10);
        assert!(!config.allow_private_addresses);
        assert_eq!(config.providers[0].allow_registration, ProviderRegistration::FollowSite);
        assert!(config.providers[0].trust_email);
        assert_eq!(config.providers[1].allow_registration, ProviderRegistration::Always);
        assert_eq!(config.providers[2].allow_registration, ProviderRegistration::Never);
        assert_eq!(config.providers[1].github_api_url(), "https://api.github.com");
        assert_eq!(config.providers[2].github_api_url(), "https://ghe.example.com/api/v3");
        assert_eq!(config.providers[2].github_base_url(), "https://ghe.example.com");
        assert!(config.validate().is_ok(), "{:?}", config.problems());
        let invalid: Result<AuthConfig, _> = serde_json::from_str(r#"{"providers": [{"allow_registration": "sometimes"}]}"#);
        assert!(invalid.is_err());
    }

    #[test]
    fn scopes_default_per_kind_and_oidc_always_asks_for_openid() {
        assert_eq!(oidc("corp").effective_scopes(), ["openid", "email", "profile"]);
        assert_eq!(github().effective_scopes(), ["read:user", "user:email"]);
        let custom = ProviderConfig { scopes: Some(vec!["email".to_owned()]), ..oidc("corp") };
        assert_eq!(custom.effective_scopes(), ["openid", "email"]);
        assert_eq!(oidc("corp").issuer_url(), Some("https://sso.example.com/realms/main"));
    }

    #[test]
    fn validation_reports_every_problem() {
        assert!(config(vec![oidc("corp"), github()]).validate().is_ok());
        let problems = config(vec![
            oidc("corp"),
            oidc("corp"),
            ProviderConfig { issuer: None, ..oidc("no-issuer") },
            ProviderConfig { client_secret: String::new(), ..oidc("Bad_Id") },
            ProviderConfig { issuer: Some("http://sso.internal".to_owned()), ..oidc("plain") },
            ProviderConfig { issuer: Some("https://sso.example.com/?x=1".to_owned()), ..oidc("query") },
            ProviderConfig { trust_mfa: true, issuer: Some("https://x.example".to_owned()), ..github() },
            oidc("two-factor"),
        ])
        .problems();
        let all = problems.join("\n");
        assert!(all.contains("\"corp\": the id is used by another provider"), "{all}");
        assert!(all.contains("`issuer` is required"), "{all}");
        assert!(all.contains("lower-case letters"), "{all}");
        assert!(all.contains("must use https"), "{all}");
        assert!(all.contains("without query"), "{all}");
        assert!(all.contains("`issuer` is for kind = \"oidc\""), "{all}");
        assert!(all.contains("`trust_mfa` needs an OIDC provider"), "{all}");
        assert!(all.contains("reserved"), "{all}");

        let mut local = config(vec![ProviderConfig { issuer: Some("http://127.0.0.1:8080".to_owned()), ..oidc("local") }]);
        assert!(local.validate().is_err());
        local.allow_private_addresses = true;
        assert!(local.validate().is_ok());
        let unnamed = config(vec![ProviderConfig { id: String::new(), name: String::new(), client_id: String::new(), ..oidc("x") }]);
        let all = unnamed.problems().join("\n");
        assert!(all.contains("auth.providers[0]: `id` is required"), "{all}");
        assert!(all.contains("`name` is required") && all.contains("`client_id` is required"), "{all}");
    }

    #[test]
    fn secrets_come_from_the_environment_by_position_or_id() {
        let mut config = config(vec![ProviderConfig { client_secret: String::new(), ..oidc("corp-sso") }, github()]);
        assert!(AuthConfig::is_provider_env_key("auth__providers__0__client_secret"));
        assert!(AuthConfig::is_provider_env_key("AUTH__PROVIDERS__CORP_SSO__CLIENT_SECRET"));
        assert!(!AuthConfig::is_provider_env_key("AUTH__TIMEOUT_SECONDS"));
        config
            .apply_provider_env([
                ("SERVER__PUBLIC_URL", "ignored"),
                ("AUTH__PROVIDERS__CORP_SSO__CLIENT_SECRET", "from-env"),
                ("auth__providers__1__client_id", "other-id"),
            ])
            .unwrap();
        assert_eq!(config.providers[0].client_secret, "from-env");
        assert_eq!(config.providers[1].client_id, "other-id");
        assert!(config.validate().is_ok());
        assert!(
            config.apply_provider_env([("AUTH__PROVIDERS__7__CLIENT_SECRET", "x")]).unwrap_err().contains("no [[auth.providers]] entry 7")
        );
        assert!(config.apply_provider_env([("AUTH__PROVIDERS__0__ISSUER", "x")]).unwrap_err().contains("only CLIENT_ID and CLIENT_SECRET"));
    }

    #[test]
    fn debug_output_never_shows_the_secret() {
        let debug = format!("{:?}", oidc("corp"));
        assert!(!debug.contains("s3cret"));
        assert!(debug.contains("<redacted>"));
    }
}
