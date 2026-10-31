//! Account security pages: two-factor authentication, sessions, sign-in second step.
//!
//! Milestone 1 workstream S owns this file.
//!
//! `/settings/security` renders one template in three modes ([`SecurityView`]): the
//! overview (two-factor methods — authenticator app and passkeys — sessions, recent
//! activity), the authenticator setup (QR code and secret) and the one-time display of
//! fresh recovery codes.
//!
//! The administrators' audit log (`/admin/audit`, [`AuditPage`]) lives here too, so all
//! security views share their formatting.

use std::fmt::Write as _;

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qrcode::{Color, QrCode};
use rbit_services::{
    accounts::ActiveSession,
    admin::Paged,
    audit::{AuditCategory, AuditEvent},
    two_factor::{Enrolment, Passkey, TwoFactorStatus},
};

use crate::views::{
    admin::{AdminLink, AdminSection},
    components::FormError,
    external_auth::SignInSection,
    layout::Page,
    settings::{SettingsLink, SettingsSection},
};

pub const SECURITY_URL: &str = SettingsSection::Security.url();

/// A confirmation shown at the top of the overview after a redirect (`?saved=…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityNotice {
    TwoFactorDisabled,
    AuthenticatorAdded,
    PasskeyAdded,
    PasskeyRemoved,
    SessionRevoked,
    OtherSessionsRevoked,
    SetupCancelled,
    // Milestone 3: external sign-in (O).
    IdentityLinked,
    IdentityUnlinked,
}

impl SecurityNotice {
    const ALL: [Self; 9] = [
        Self::TwoFactorDisabled,
        Self::AuthenticatorAdded,
        Self::PasskeyAdded,
        Self::PasskeyRemoved,
        Self::SessionRevoked,
        Self::OtherSessionsRevoked,
        Self::SetupCancelled,
        Self::IdentityLinked,
        Self::IdentityUnlinked,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::TwoFactorDisabled => "two-factor-disabled",
            Self::AuthenticatorAdded => "authenticator-added",
            Self::PasskeyAdded => "passkey-added",
            Self::PasskeyRemoved => "passkey-removed",
            Self::SessionRevoked => "session-revoked",
            Self::OtherSessionsRevoked => "sessions-revoked",
            Self::SetupCancelled => "setup-cancelled",
            Self::IdentityLinked => "identity-linked",
            Self::IdentityUnlinked => "identity-unlinked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|notice| notice.key() == value)
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::TwoFactorDisabled => "Authenticator app removed.",
            Self::AuthenticatorAdded => "Authenticator app added.",
            Self::PasskeyAdded => "Passkey added.",
            Self::PasskeyRemoved => "Passkey removed.",
            Self::SessionRevoked => "Session signed out.",
            Self::OtherSessionsRevoked => "Signed out of every other session.",
            Self::SetupCancelled => "Two-factor setup cancelled.",
            Self::IdentityLinked => "Account linked. You can sign in with it now.",
            Self::IdentityUnlinked => "Account unlinked.",
        }
    }

    /// `/settings/security?saved=…`
    pub fn url(self) -> String {
        format!("{SECURITY_URL}?saved={}", self.key())
    }
}

/// Two-factor state for the overview.
#[derive(Debug, Clone)]
pub struct TwoFactorSummary {
    /// Any factor (authenticator app or passkey) protects the account.
    pub enabled: bool,
    /// The authenticator app is set up.
    pub authenticator: bool,
    /// When the authenticator app was set up, e.g. `Enabled Oct 15, 2026`.
    pub since: String,
    /// e.g. `9 recovery codes left`.
    pub recovery_codes: String,
    pub recovery_codes_low: bool,
}

impl From<&TwoFactorStatus> for TwoFactorSummary {
    fn from(status: &TwoFactorStatus) -> Self {
        let left = status.recovery_codes_left;
        Self {
            enabled: status.is_enabled(),
            authenticator: status.has_authenticator(),
            since: status.enabled_at.map(|at| format!("Enabled {}", at.date())).unwrap_or_default(),
            recovery_codes: format!("{left} recovery code{} left", if left == 1 { "" } else { "s" }),
            recovery_codes_low: left <= 2,
        }
    }
}

/// One registered passkey.
#[derive(Debug, Clone)]
pub struct PasskeyRow {
    pub id: i64,
    pub name: String,
    /// e.g. `Added Oct 15, 2026`.
    pub added: String,
    /// e.g. `Last used 3 days ago`, or `Never used`.
    pub last_used: String,
    pub remove_url: String,
    /// A failed removal of this passkey.
    pub error: FormError,
}

impl From<&Passkey> for PasskeyRow {
    fn from(passkey: &Passkey) -> Self {
        Self {
            id: passkey.id,
            name: passkey.name.clone(),
            added: format!("Added {}", passkey.created_at.date()),
            last_used: passkey.last_used_at.map_or_else(|| "Never used".to_owned(), |at| format!("Last used {}", at.relative())),
            remove_url: format!("{SECURITY_URL}/passkeys/{}/remove", passkey.id),
            error: FormError::default(),
        }
    }
}

/// One session in the list.
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// e.g. `Firefox on Windows`.
    pub device: String,
    pub is_mobile: bool,
    pub ip_address: Option<String>,
    /// e.g. `Signed in Oct 15, 2026`.
    pub signed_in: String,
    pub signed_in_iso: String,
    /// e.g. `Active 3 hours ago`, or `Active now` for the current session.
    pub last_active: String,
    pub is_current: bool,
    pub revoke_url: String,
}

impl From<&ActiveSession> for SessionRow {
    fn from(session: &ActiveSession) -> Self {
        let agent = UserAgent::parse(&session.user_agent);
        let last_active = if session.is_current {
            "Active now".to_owned()
        } else {
            match session.last_seen_at.relative().as_str() {
                "just now" => "Active just now".to_owned(),
                relative => format!("Active {relative}"),
            }
        };
        Self {
            device: agent.summary(),
            is_mobile: agent.mobile,
            ip_address: session.ip_address.clone(),
            signed_in: format!("Signed in {}", session.created_at.date()),
            signed_in_iso: session.created_at.iso8601(),
            last_active,
            is_current: session.is_current,
            revoke_url: format!("{SECURITY_URL}/sessions/{}/revoke", session.id),
        }
    }
}

/// One line of "Recent activity".
#[derive(Debug, Clone)]
pub struct ActivityRow {
    pub label: &'static str,
    pub detail: String,
    pub when: String,
    pub when_iso: String,
    pub ip_address: Option<String>,
    pub is_failure: bool,
}

impl From<&AuditEvent> for ActivityRow {
    fn from(event: &AuditEvent) -> Self {
        Self {
            label: event.action.label(),
            detail: event.detail.clone(),
            when: event.created_at.relative(),
            when_iso: event.created_at.iso8601(),
            ip_address: event.ip_address.clone(),
            is_failure: event.action.is_failure(),
        }
    }
}

/// The overview: two-factor methods, sessions and recent activity.
#[derive(Debug, Clone)]
pub struct Overview {
    pub two_factor: TwoFactorSummary,
    pub passkeys: Vec<PasskeyRow>,
    /// `navigator.credentials.create` options for adding a passkey (JSON), unless passkeys
    /// can't work on this site (no usable `server.public_url`).
    pub passkey_create_options: Option<String>,
    /// `navigator.credentials.get` options (JSON) for confirming the forms below with a
    /// passkey instead of a code; only when the account has passkeys.
    pub passkey_confirm_options: Option<String>,
    pub notice: Option<SecurityNotice>,
    pub add_passkey_error: FormError,
    pub disable_error: FormError,
    pub regenerate_error: FormError,
    pub sessions: Vec<SessionRow>,
    pub activity: Vec<ActivityRow>,
    /// Milestone 3 (O): password and linked identities.
    pub sign_in: Box<SignInSection>,
}

impl Overview {
    pub fn new(status: &TwoFactorStatus, sessions: &[ActiveSession], activity: &[AuditEvent]) -> Self {
        Self {
            two_factor: status.into(),
            passkeys: status.passkeys.iter().map(PasskeyRow::from).collect(),
            passkey_create_options: None,
            passkey_confirm_options: None,
            notice: None,
            add_passkey_error: FormError::default(),
            disable_error: FormError::default(),
            regenerate_error: FormError::default(),
            sessions: sessions.iter().map(SessionRow::from).collect(),
            activity: activity.iter().map(ActivityRow::from).collect(),
            sign_in: Box::default(),
        }
    }

    pub fn has_other_sessions(&self) -> bool {
        self.sessions.iter().any(|session| !session.is_current)
    }
}

/// The two-factor setup step: scan, then confirm with a code.
#[derive(Debug, Clone)]
pub struct Setup {
    /// Inline SVG (generated here, never from user input).
    pub qr_svg: String,
    /// The base32 secret in groups of four, for manual entry.
    pub secret: String,
    pub error: FormError,
}

impl Setup {
    pub fn new(enrolment: &Enrolment, error: FormError) -> Self {
        Self { qr_svg: qr_svg(&enrolment.provisioning_uri), secret: group(&enrolment.secret, 4), error }
    }
}

/// Freshly issued recovery codes, shown exactly once.
#[derive(Debug, Clone)]
pub struct RecoveryCodes {
    pub codes: Vec<String>,
    /// `true` right after enabling 2FA, `false` after regenerating.
    pub just_enabled: bool,
}

#[derive(Debug, Clone)]
pub enum SecurityView {
    Overview(Overview),
    Setup(Setup),
    RecoveryCodes(RecoveryCodes),
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/security.html")]
pub struct SecurityPage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub view: SecurityView,
}

impl SecurityPage {
    pub fn new(page: Page, view: SecurityView) -> Self {
        let title = match view {
            SecurityView::Overview(_) => "Security",
            SecurityView::Setup(_) => "Set up an authenticator app",
            SecurityView::RecoveryCodes(_) => "Recovery codes",
        };
        Self { page: page.titled(title), settings_nav: SettingsSection::Security.nav(), view }
    }
}

// ---------------------------------------------------------------------- audit log (administrators)

const AUDIT_URL: &str = AdminSection::AuditLog.url();

/// `/admin/audit?q=…&category=…&page=…`, omitting defaults.
pub fn audit_url(search: &str, category: Option<AuditCategory>, page: u32) -> String {
    let mut params = Vec::new();
    if !search.is_empty() {
        params.push(format!("q={}", utf8_percent_encode(search, NON_ALPHANUMERIC)));
    }
    if let Some(category) = category {
        params.push(format!("category={}", category.as_str()));
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() { AUDIT_URL.to_owned() } else { format!("{AUDIT_URL}?{}", params.join("&")) }
}

/// One `<option>` of the category filter.
#[derive(Debug, Clone)]
pub struct CategoryOption {
    pub value: &'static str,
    pub label: &'static str,
    pub selected: bool,
}

/// Search, filter and pagination state (the fields `admin/components.html`'s `pager` reads).
#[derive(Debug, Clone)]
pub struct AuditListing {
    pub search: String,
    pub categories: Vec<CategoryOption>,
    /// e.g. `1–30 of 112`, or `No events`.
    pub summary: String,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
    pub is_filtered: bool,
}

impl AuditListing {
    pub fn has_pages(&self) -> bool {
        self.previous_url.is_some() || self.next_url.is_some()
    }
}

/// One entry of the log.
#[derive(Debug, Clone)]
pub struct AuditRow {
    pub label: &'static str,
    pub category: &'static str,
    pub is_failure: bool,
    pub actor: Option<String>,
    /// The actor is only a login someone tried (failed sign-in), not a verified account.
    pub actor_unverified: bool,
    pub target: Option<String>,
    pub ip_address: Option<String>,
    pub detail: String,
    /// e.g. `Oct 15, 2026`, with the relative time as a tooltip.
    pub date: String,
    pub relative: String,
    pub iso: String,
}

impl From<&AuditEvent> for AuditRow {
    fn from(event: &AuditEvent) -> Self {
        Self {
            label: event.action.label(),
            category: event.action.category().label(),
            is_failure: event.action.is_failure(),
            actor: event.actor_name.clone(),
            actor_unverified: event.actor_id.is_none(),
            target: event.target_name.clone(),
            ip_address: event.ip_address.clone(),
            detail: event.detail.clone(),
            date: event.created_at.date(),
            relative: event.created_at.relative(),
            iso: event.created_at.iso8601(),
        }
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/audit.html")]
pub struct AuditPage {
    pub page: Page,
    pub admin_nav: Vec<AdminLink>,
    pub listing: AuditListing,
    pub events: Vec<AuditRow>,
}

impl AuditPage {
    pub fn new(page: Page, search: &str, category: Option<AuditCategory>, paged: &Paged<AuditEvent>) -> Self {
        let summary = if paged.total == 0 {
            "No events".to_owned()
        } else {
            format!("{}–{} of {}", paged.first_index(), paged.last_index(), paged.total)
        };
        let categories = std::iter::once(CategoryOption { value: "", label: "All events", selected: category.is_none() })
            .chain(AuditCategory::ALL.into_iter().map(|option| CategoryOption {
                value: option.as_str(),
                label: option.label(),
                selected: category == Some(option),
            }))
            .collect();
        Self {
            page: page.titled(crate::i18n::text("core", "title-audit-log")),
            admin_nav: AdminSection::AuditLog.nav(),
            listing: AuditListing {
                search: search.to_owned(),
                categories,
                summary,
                previous_url: paged.has_previous().then(|| audit_url(search, category, paged.page - 1)),
                next_url: paged.has_next().then(|| audit_url(search, category, paged.page + 1)),
                is_filtered: !search.is_empty() || category.is_some(),
            },
            events: paged.items.iter().map(AuditRow::from).collect(),
        }
    }
}

// ---------------------------------------------------------------------- helpers

/// `ABCDEFGH…` → `ABCD EFGH …`.
fn group(text: &str, size: usize) -> String {
    text.as_bytes().chunks(size).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect::<Vec<_>>().join(" ")
}

/// Modules of light margin around the code, as the QR specification requires.
const QUIET_ZONE: usize = 4;

/// The QR code for `data` as a self-contained SVG: black modules on a white square in
/// both colour schemes (scanners struggle with inverted codes), one `<path>` built from
/// horizontal runs. Presentation attributes only, so it passes the strict CSP.
pub fn qr_svg(data: &str) -> String {
    let Ok(code) = QrCode::new(data.as_bytes()) else {
        return String::new();
    };
    let width = code.width();
    let colors = code.to_colors();
    let size = width + 2 * QUIET_ZONE;
    let mut path = String::new();
    for (y, row) in colors.chunks(width).enumerate() {
        let mut x = 0;
        while x < width {
            if row[x] == Color::Dark {
                let start = x;
                while x < width && row[x] == Color::Dark {
                    x += 1;
                }
                let _ = write!(path, "M{} {}h{}v1h-{}z", start + QUIET_ZONE, y + QUIET_ZONE, x - start, x - start);
            } else {
                x += 1;
            }
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" width=\"100%\" height=\"100%\" role=\"img\" \
         aria-label=\"QR code for your authenticator app\" shape-rendering=\"crispEdges\">\
         <rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/><path fill=\"#000\" d=\"{path}\"/></svg>"
    )
}

/// The parts of a `User-Agent` worth showing in a session list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UserAgent {
    browser: Option<&'static str>,
    os: Option<&'static str>,
    mobile: bool,
}

impl UserAgent {
    /// Recognises the common browsers and systems; order matters because user agents
    /// name their ancestors (Edge says Chrome and Safari, Chrome says Safari, Android says Linux).
    fn parse(agent: &str) -> Self {
        const BROWSERS: [(&str, &str); 9] = [
            ("Edg/", "Edge"),
            ("OPR/", "Opera"),
            ("Vivaldi/", "Vivaldi"),
            ("Firefox/", "Firefox"),
            ("FxiOS/", "Firefox"),
            ("CriOS/", "Chrome"),
            ("Chrome/", "Chrome"),
            ("Safari/", "Safari"),
            ("git/", "Git"),
        ];
        const SYSTEMS: [(&str, &str); 7] = [
            ("Windows", "Windows"),
            ("Android", "Android"),
            ("iPhone", "iOS"),
            ("iPad", "iPadOS"),
            ("Mac OS X", "macOS"),
            ("CrOS", "ChromeOS"),
            ("Linux", "Linux"),
        ];
        let find = |table: &[(&str, &'static str)]| table.iter().find(|(needle, _)| agent.contains(needle)).map(|(_, name)| *name);
        Self {
            browser: find(&BROWSERS),
            os: find(&SYSTEMS),
            mobile: ["Mobile", "iPhone", "Android"].iter().any(|needle| agent.contains(needle)),
        }
    }

    fn summary(self) -> String {
        match (self.browser, self.os) {
            (Some(browser), Some(os)) => format!("{browser} on {os}"),
            (Some(browser), None) => browser.to_owned(),
            (None, Some(os)) => format!("Unknown browser on {os}"),
            (None, None) => "Unknown device".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::Timestamp;
    use rbit_services::audit::AuditAction;

    use super::*;

    fn page() -> Page {
        Page {
            locale: rbit_messages::LocaleSnapshot::english(0),
            has_i18n: false,
            updates: rbit_services::updates::UpdateStatus::default(),
            site_name: "rbit".to_owned(),
            plugin_revision: 0,
            branding: rbit_core::config::BrandingConfig::default(),
            title: None,
            viewer: None,
            path: SECURITY_URL.to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    fn session(id: &str, agent: &str, is_current: bool) -> ActiveSession {
        ActiveSession {
            id: id.to_owned(),
            created_at: Timestamp(1_792_022_400),
            last_seen_at: Timestamp::now(),
            expires_at: Timestamp(1_800_000_000),
            ip_address: Some("192.0.2.4".to_owned()),
            user_agent: agent.to_owned(),
            is_current,
        }
    }

    const FIREFOX_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0";
    const SAFARI_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
    const EDGE: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
    const CHROME_ANDROID: &str =
        "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36";

    #[test]
    fn summarises_user_agents() {
        assert_eq!(UserAgent::parse(FIREFOX_WINDOWS).summary(), "Firefox on Windows");
        assert_eq!(UserAgent::parse(SAFARI_IPHONE).summary(), "Safari on iOS");
        assert!(UserAgent::parse(SAFARI_IPHONE).mobile);
        assert_eq!(UserAgent::parse(EDGE).summary(), "Edge on Windows");
        assert_eq!(UserAgent::parse(CHROME_ANDROID).summary(), "Chrome on Android");
        assert_eq!(UserAgent::parse("").summary(), "Unknown device");
        assert!(!UserAgent::parse(FIREFOX_WINDOWS).mobile);
    }

    #[test]
    fn qr_codes_are_plain_svg() {
        let svg = qr_svg("otpauth://totp/rbit:alice?secret=GEZDGNBVGY3TQOJQ&issuer=rbit");
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<path fill=\"#000\" d=\"M"));
        assert!(!svg.contains("style="), "no inline styles under the CSP");
        assert!(!svg.contains("<?xml"));
    }

    #[test]
    fn groups_secrets() {
        assert_eq!(group("ABCDEFGHIJ", 4), "ABCD EFGH IJ");
    }

    #[test]
    fn notices_round_trip() {
        for notice in SecurityNotice::ALL {
            assert_eq!(SecurityNotice::parse(notice.key()), Some(notice));
            assert!(notice.url().starts_with("/settings/security?saved="));
        }
        assert_eq!(SecurityNotice::parse("nope"), None);
    }

    #[test]
    fn overview_lists_sessions_and_marks_the_current_one() {
        let status = TwoFactorStatus { enabled_at: None, passkeys: Vec::new(), recovery_codes_left: 0 };
        let sessions = [session("aaaa", FIREFOX_WINDOWS, true), session("bbbb", SAFARI_IPHONE, false)];
        let event = AuditEvent {
            id: 1,
            created_at: Timestamp::now(),
            action: AuditAction::SignInFailed,
            actor_id: None,
            actor_name: Some("alice".to_owned()),
            target_name: None,
            ip_address: Some("203.0.113.9".to_owned()),
            detail: "wrong username or password".to_owned(),
        };
        let mut overview = Overview::new(&status, &sessions, &[event]);
        overview.notice = Some(SecurityNotice::SessionRevoked);
        let html = SecurityPage::new(page(), SecurityView::Overview(overview)).render().unwrap();

        assert!(html.contains("Firefox on Windows"));
        assert!(html.contains("Safari on iOS"));
        assert!(html.contains("This device"));
        assert!(html.contains("Active now"));
        assert!(html.contains(r#"action="/settings/security/sessions/bbbb/revoke""#));
        assert!(!html.contains("/settings/security/sessions/aaaa/revoke"), "the current session signs out via the header");
        assert!(html.contains(r#"action="/settings/security/sessions/revoke-others""#));
        assert!(html.contains("Session signed out."));
        assert!(html.contains(r#"href="/settings/security/two-factor""#), "offers setup");
        assert!(html.contains("Failed sign-in"));
        assert!(html.contains("203.0.113.9"));
        assert!(html.contains(r#"aria-current="page""#));
    }

    #[test]
    fn overview_with_two_factor_offers_disable_and_regenerate() {
        let status = TwoFactorStatus { enabled_at: Some(Timestamp(1_792_022_400)), passkeys: Vec::new(), recovery_codes_left: 1 };
        let mut overview = Overview::new(&status, &[session("aaaa", "", true)], &[]);
        overview.disable_error = FormError::new("Incorrect password or code.");
        let html = SecurityPage::new(page(), SecurityView::Overview(overview)).render().unwrap();
        assert!(html.contains("Enabled Oct 15, 2026"));
        assert!(html.contains("1 recovery code left"));
        assert!(html.contains(r#"action="/settings/security/two-factor/disable""#));
        assert!(html.contains(r#"action="/settings/security/recovery-codes""#));
        assert!(html.contains("Incorrect password or code."));
        assert!(!html.contains("revoke-others"), "no other sessions to sign out");
    }

    #[test]
    fn overview_lists_passkeys_and_offers_them_for_confirmation() {
        let passkey = Passkey { id: 7, name: "YubiKey <5>".to_owned(), created_at: Timestamp(1_792_022_400), last_used_at: None };
        let status = TwoFactorStatus { enabled_at: None, passkeys: vec![passkey], recovery_codes_left: 10 };
        let mut overview = Overview::new(&status, &[session("aaaa", "", true)], &[]);
        overview.passkey_create_options = Some(r#"{"challenge":"abc"}"#.to_owned());
        overview.passkey_confirm_options = Some(r#"{"challenge":"def"}"#.to_owned());
        overview.passkeys[0].error = FormError::new("Incorrect password or code.");
        let html = SecurityPage::new(page(), SecurityView::Overview(overview)).render().unwrap();

        assert!(!html.contains("YubiKey <5>"), "names are escaped");
        assert!(html.contains("Never used"));
        assert!(html.contains(r#"action="/settings/security/passkeys/7/remove""#));
        assert!(html.contains(r#"action="/settings/security/passkeys""#));
        assert!(html.contains("data-passkey-create="), "the add form carries creation options");
        assert!(html.contains("data-passkey-get="), "confirmations can use a passkey");
        assert!(!html.contains(r#"{"challenge""#), "options are attribute-escaped");
        assert!(html.contains("Incorrect password or code."));
        assert!(html.contains("10 recovery codes left"));
        assert!(html.contains(r#"href="/settings/security/two-factor""#), "the authenticator app can still be added");
        assert!(!html.contains(r#"action="/settings/security/two-factor/disable""#), "no app to remove");
    }

    #[test]
    fn setup_shows_qr_code_and_secret() {
        let enrolment = Enrolment {
            secret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
            provisioning_uri: "otpauth://totp/rbit:alice?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
        };
        let html = SecurityPage::new(page(), SecurityView::Setup(Setup::new(&enrolment, FormError::default()))).render().unwrap();
        assert!(html.contains("<svg xmlns"), "the QR code is not escaped");
        assert!(html.contains("GEZD GNBV GY3T QOJQ GEZD GNBV GY3T QOJQ"));
        assert!(html.contains(r#"action="/settings/security/two-factor""#));
        assert!(html.contains(r#"autocomplete="one-time-code""#));
        assert!(html.contains(r#"action="/settings/security/two-factor/cancel""#));
    }

    fn event(id: i64, action: AuditAction, actor: Option<(i64, &str)>) -> AuditEvent {
        AuditEvent {
            id,
            created_at: Timestamp(1_792_022_400),
            action,
            actor_id: actor.map(|(id, _)| rbit_core::AccountId(id)),
            actor_name: actor.map(|(_, name)| name.to_owned()),
            target_name: Some("mallory".to_owned()),
            ip_address: Some("198.51.100.23".to_owned()),
            detail: String::new(),
        }
    }

    #[test]
    fn audit_urls_keep_filters() {
        assert_eq!(audit_url("", None, 1), "/admin/audit");
        assert_eq!(audit_url("a b", Some(AuditCategory::TwoFactor), 3), "/admin/audit?q=a%20b&category=two_factor&page=3");
    }

    #[test]
    fn audit_page_renders_events_filters_and_paging() {
        let mut failed = event(1, AuditAction::SignInFailed, Some((0, "ghost")));
        failed.actor_id = None;
        let paged =
            Paged { items: vec![event(2, AuditAction::UserSuspended, Some((1, "root"))), failed], total: 32, page: 2, per_page: 30 };
        let html = AuditPage::new(page(), "mall", Some(AuditCategory::Administration), &paged).render().unwrap();

        assert!(html.contains("Suspended a user"));
        assert!(html.contains(r#"href="/root""#), "verified actors link to their profile");
        assert!(html.contains("as <span class=\"font-mono\">ghost</span>"), "attempted logins are not links");
        assert!(html.contains("198.51.100.23"));
        assert!(html.contains(r#"<option value="administration" selected>"#));
        assert!(html.contains(r#"value="mall""#));
        assert!(html.contains("31–32 of 32"));
        assert!(html.contains("rel=\"prev\""));
        assert!(html.contains("q=mall") && html.contains("category=administration"), "paging keeps the filters");
        assert!(html.contains(r#"href="/admin/audit""#), "clear filters");
        assert!(html.contains("text-danger"), "failures stand out");
        assert!(html.contains(r#"aria-current="page""#));
    }

    #[test]
    fn empty_audit_page() {
        let paged = Paged { items: Vec::new(), total: 0, page: 1, per_page: 30 };
        let html = AuditPage::new(page(), "", None, &paged).render().unwrap();
        assert!(html.contains("No events"));
        assert!(!html.contains("rel=\"next\""));
    }

    #[test]
    fn recovery_codes_are_listed_once() {
        let codes = RecoveryCodes { codes: vec!["abcd-efgh-jkmn".to_owned(), "pqrs-tvwx-yz01".to_owned()], just_enabled: true };
        let html = SecurityPage::new(page(), SecurityView::RecoveryCodes(codes)).render().unwrap();
        assert!(html.contains("abcd-efgh-jkmn"));
        assert!(html.contains("pqrs-tvwx-yz01"));
        assert!(html.contains("Two-factor authentication is on"));
        assert!(html.contains(r#"hx-history="false""#), "kept out of htmx's history cache");
    }
}
