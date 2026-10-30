//! External sign-in pages: the provider buttons on the sign-in page, the "choose a
//! username" step after a first sign-in, the page that explains a refusal, and the
//! "Sign-in methods" section of Settings → Security.
//!
//! Milestone 3 workstream O (external sign-in) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_services::external_auth::{ProviderSummary, RegistrationDraft, SignInMethods};

use crate::views::{components::FormError, layout::Page};

/// `GET /login/{id}`: starts a sign-in.
pub fn sign_in_url(provider_id: &str, next: &str) -> String {
    if next == "/" {
        format!("/login/{provider_id}")
    } else {
        format!("/login/{provider_id}?next={}", utf8_percent_encode(next, NON_ALPHANUMERIC))
    }
}

/// `/settings/security/linked/{id}?token=…`: starts a link (the token binds it to the session).
pub fn link_url(provider_id: &str, token: &str) -> String {
    format!("/settings/security/linked/{provider_id}?token={}", utf8_percent_encode(token, NON_ALPHANUMERIC))
}

/// `POST` here to unlink.
pub fn unlink_url(provider_id: &str) -> String {
    format!("/settings/security/linked/{provider_id}/unlink")
}

/// `/login/{id}/register`: the "choose a username" step.
pub fn register_url(provider_id: &str) -> String {
    format!("/login/{provider_id}/register")
}

/// "Sign in with …" on the sign-in page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderButton {
    pub name: String,
    pub url: String,
}

impl ProviderButton {
    pub fn all(providers: &[ProviderSummary], next: &str) -> Vec<Self> {
        providers.iter().map(|provider| Self { name: provider.name.clone(), url: sign_in_url(&provider.id, next) }).collect()
    }
}

/// After a first sign-in with a provider: choose a username.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/external_register.html")]
pub struct RegisterPage {
    pub page: Page,
    pub error: FormError,
    pub provider_name: String,
    pub action: String,
    pub username: String,
    pub email: String,
    /// The provider vouches for the pre-filled address.
    pub email_verified: bool,
}

impl RegisterPage {
    pub fn new(page: Page, draft: &RegistrationDraft, error: FormError) -> Self {
        Self {
            page: page.titled("Create your account"),
            error,
            provider_name: draft.provider_name.clone(),
            action: register_url(&draft.provider_id),
            username: draft.username.clone(),
            email: draft.email.clone(),
            email_verified: draft.email_verified,
        }
    }
}

/// Why an external sign-in (or link) didn't go through.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "auth/external_refused.html")]
pub struct RefusedPage {
    pub page: Page,
    pub message: String,
    pub back_url: &'static str,
    pub back_label: &'static str,
}

impl RefusedPage {
    /// `linking`: the flow started in settings, so that is where "back" goes.
    pub fn new(page: Page, message: &str, linking: bool) -> Self {
        let (back_url, back_label) =
            if linking { ("/settings/security", "Back to security settings") } else { ("/login", "Back to sign in") };
        Self {
            page: page.titled("Sign-in didn't work"),
            message: FormError::from_message(message).0.unwrap_or_default(),
            back_url,
            back_label,
        }
    }
}

/// One linked identity in settings.
#[derive(Debug, Clone)]
pub struct IdentityRow {
    pub provider_name: String,
    /// e.g. `alice · alice@example.com`.
    pub detail: String,
    /// e.g. `Linked Oct 15, 2026`.
    pub linked: String,
    /// e.g. `Last used 3 days ago`.
    pub last_used: String,
    /// The provider was removed from the configuration: the identity can't sign in.
    pub configured: bool,
    pub unlink_url: String,
    /// `false` when it is the account's only way to sign in.
    pub can_unlink: bool,
}

/// A configured provider not linked yet.
#[derive(Debug, Clone)]
pub struct LinkRow {
    pub name: String,
    pub url: String,
}

/// The "Sign-in methods" section of Settings → Security.
#[derive(Debug, Clone, Default)]
pub struct SignInSection {
    /// Shown only when providers are configured or identities are linked.
    pub shown: bool,
    pub has_password: bool,
    pub identities: Vec<IdentityRow>,
    pub linkable: Vec<LinkRow>,
    pub error: FormError,
}

impl SignInSection {
    /// `link_token` authorises the "Link" URLs for the current session.
    pub fn new(methods: &SignInMethods, link_token: &str) -> Self {
        let identities = methods
            .identities
            .iter()
            .map(|identity| IdentityRow {
                provider_name: identity.provider_name.clone(),
                detail: [identity.username.as_deref(), identity.email.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                linked: format!("Linked {}", identity.created_at.date()),
                last_used: identity.last_used_at.map_or_else(|| "Never used".to_owned(), |at| format!("Last used {}", at.relative())),
                configured: identity.configured,
                unlink_url: unlink_url(&identity.provider_id),
                can_unlink: !methods.is_last_method(&identity.provider_id),
            })
            .collect::<Vec<_>>();
        let linkable = methods
            .linkable
            .iter()
            .map(|provider| LinkRow { name: provider.name.clone(), url: link_url(&provider.id, link_token) })
            .collect::<Vec<_>>();
        Self {
            shown: !identities.is_empty() || !linkable.is_empty(),
            has_password: methods.has_password,
            identities,
            linkable,
            error: FormError::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{Timestamp, config::ProviderKind};
    use rbit_services::external_auth::LinkedIdentity;

    use super::*;

    fn page() -> Page {
        Page {
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            plugin_revision: 0,
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: "/login".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    #[test]
    fn urls_carry_next_and_tokens() {
        assert_eq!(sign_in_url("corp", "/"), "/login/corp");
        assert_eq!(sign_in_url("corp", "/a/b?x=1"), "/login/corp?next=%2Fa%2Fb%3Fx%3D1");
        assert_eq!(link_url("github", "ab-_"), "/settings/security/linked/github?token=ab%2D%5F");
        let providers = [ProviderSummary { id: "corp".to_owned(), name: "Corp SSO".to_owned(), kind: ProviderKind::Oidc }];
        assert_eq!(ProviderButton::all(&providers, "/"), [ProviderButton { name: "Corp SSO".to_owned(), url: "/login/corp".to_owned() }]);
    }

    #[test]
    fn register_page_prefills_the_provider_details() {
        let draft = RegistrationDraft {
            provider_id: "corp".to_owned(),
            provider_name: "Corp SSO".to_owned(),
            username: "alice".to_owned(),
            email: "alice@corp.example".to_owned(),
            email_verified: true,
            next: "/".to_owned(),
        };
        let html = RegisterPage::new(page(), &draft, FormError::default()).render().unwrap();
        assert!(html.contains(r#"action="/login/corp/register""#));
        assert!(html.contains(r#"value="alice""#) && html.contains(r#"value="alice@corp.example""#));
        assert!(html.contains("Verified by Corp SSO"));
        assert!(html.contains("personal access token"), "explains Git over HTTPS without a password");
        let html =
            RegisterPage::new(page(), &RegistrationDraft { email_verified: false, ..draft }, FormError::new("Taken.")).render().unwrap();
        assert!(!html.contains("Verified by"));
        assert!(html.contains("Taken."));
    }

    #[test]
    fn refusals_say_why_and_where_to_go() {
        let html = RefusedPage::new(page(), "no account here is linked to this GitHub account", false).render().unwrap();
        assert!(html.contains("No account here is linked to this GitHub account."));
        assert!(html.contains(r#"href="/login""#));
        let html = RefusedPage::new(page(), "x", true).render().unwrap();
        assert!(html.contains(r#"href="/settings/security""#));
    }

    #[test]
    fn the_settings_section_lists_identities_and_guards_the_last_one() {
        let identity = |id: &str| LinkedIdentity {
            provider_id: id.to_owned(),
            provider_name: id.to_uppercase(),
            configured: true,
            username: Some("alice".to_owned()),
            email: Some("alice@example.com".to_owned()),
            created_at: Timestamp(1_700_000_000),
            last_used_at: None,
        };
        let linkable = vec![ProviderSummary { id: "github".to_owned(), name: "GitHub".to_owned(), kind: ProviderKind::Github }];
        let methods = SignInMethods { has_password: false, identities: vec![identity("corp")], linkable };
        let section = SignInSection::new(&methods, "tok");
        assert!(section.shown);
        assert_eq!(section.identities[0].detail, "alice · alice@example.com");
        assert!(!section.identities[0].can_unlink, "the only way in");
        assert_eq!(section.linkable[0].url, "/settings/security/linked/github?token=tok");
        let empty = SignInSection::new(&SignInMethods { has_password: true, identities: Vec::new(), linkable: Vec::new() }, "tok");
        assert!(!empty.shown, "nothing to show without providers");
    }
}
