//! The administrator configuration page: section, field and choice text by schema key.
//!
//! The web schema names each section and field by its configuration key; the English
//! text lives here, keyed by section and field because field keys repeat across
//! sections. Sign-in provider cards use the section key `provider`.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

/// Declares the label and help of every field of some sections.
macro_rules! config_fields {
    ($($section:literal { $($key:literal => $label:literal, $help:literal;)+ })+) => {
        /// The label and help of `key` in `section`, with their catalog ids.
        pub(super) fn field(section: &str, key: &str) -> Option<super::Field> {
            Some(match (section, key) {
                $($(($section, $key) => super::Field {
                    label_id: concat!("admin.configuration.", $section, ".", $key, ".label"),
                    label: $label,
                    help_id: concat!("admin.configuration.", $section, ".", $key, ".help"),
                    help: $help,
                },)+)+
                _ => return None,
            })
        }

        /// Every section and field key of this table.
        pub(super) const KEYS: &[(&str, &str)] = &[$($(($section, $key),)+)+];
    };
}

mod fields_a;
mod fields_b;

/// The catalog text of one field.
struct Field {
    label_id: &'static str,
    label: &'static str,
    help_id: &'static str,
    help: &'static str,
}

fn field(section: &str, key: &str) -> Option<Field> {
    fields_a::field(section, key).or_else(|| fields_b::field(section, key))
}

/// Declares the title and help of every section.
macro_rules! config_sections {
    ($($key:literal => $title:literal, $help:literal;)+) => {
        /// The title and help of a section, with their catalog ids.
        fn section(key: &str) -> Option<[&'static str; 4]> {
            Some(match key {
                $($key => [
                    concat!("admin.configuration.", $key, ".title"),
                    $title,
                    concat!("admin.configuration.", $key, ".help"),
                    $help,
                ],)+
                _ => return None,
            })
        }

        const SECTIONS: &[&str] = &[$($key,)+];
    };
}

config_sections! {
    "updates" => "Software updates and privacy",
        "Independent background release checks and running-version disclosure controls. Changes take effect after restart.";
    "server" => "Server",
        "Public address, listener and instance defaults.";
    "storage" => "Storage",
        "View the data directory. Relocate storage offline using deployment configuration.";
    "security" => "Security",
        "Sessions, passwords, rate limiting and trusted reverse proxies.";
    "repository" => "Repositories",
        "Defaults and limits for Git repositories.";
    "mail" => "Email delivery",
        "Configure sender identity, SMTP connection and delivery timeouts.";
    "ssh" => "SSH",
        "Built-in Git SSH listener, public clone address and connection limits.";
    "auth" => "Sign-in providers",
        "Connect OIDC and GitHub identity providers.";
    "branding" => "Branding defaults",
        "Deployment defaults. Use Branding and policies for immediate instance-wide changes.";
    "notifications" => "Email notifications",
        "Enable notification email and choose the default subscription policy.";
    "operations" => "Operations",
        "Metrics, maintenance, background jobs and caches.";
    "api" => "REST API",
        "API availability and pagination limits.";
    "webhooks" => "Webhooks",
        "Outgoing delivery, retry and retention settings.";
    "releases" => "Releases",
        "Release asset sizes, counts and upload timeouts.";
    "mirrors" => "Pull mirrors",
        "Fetch scheduling, timeouts and network access.";
    "push_mirrors" => "Push mirrors",
        "Push scheduling, timeouts and network access.";
    "imports" => "Imports",
        "Repository imports, concurrency and author matching.";
    "issues" => "Issues and inbox",
        "Commit scanning and in-app notification retention.";
    "pulls" => "Pull requests",
        "Merge limits, background checks and author addresses.";
    "editor" => "Browser editor",
        "Browser upload size, count and timeout limits.";
    "lfs" => "Git LFS",
        "Large-file storage, object sizes and token lifetime.";
    "actions" => "Actions",
        "CI/CD on external runners, retention, limits and timeouts.";
}

/// Declares the label of every option of a choice field.
macro_rules! config_choices {
    ($($section:literal $key:literal $value:literal => $label:literal;)+) => {
        /// The label of `value` in a choice field, with its catalog id.
        fn choice(section: &str, key: &str, value: &str) -> Option<[&'static str; 2]> {
            Some(match (section, key, value) {
                $(($section, $key, $value) => [concat!("admin.configuration.", $section, ".", $key, ".", $value), $label],)+
                _ => return None,
            })
        }

        const CHOICES: &[(&str, &str, &str)] = &[$(($section, $key, $value),)+];
    };
}

config_choices! {
    "mail" "transport" "disabled" => "Disabled";
    "mail" "transport" "log" => "Development log";
    "mail" "transport" "smtp" => "SMTP server";
    "mail" "smtp_security" "starttls" => "STARTTLS (recommended)";
    "mail" "smtp_security" "tls" => "Implicit TLS";
    "mail" "smtp_security" "none" => "No encryption";
    "notifications" "default_email" "off" => "Off";
    "notifications" "default_email" "participating" => "Threads the user participates in";
    "notifications" "default_email" "all" => "All subscribed threads";
    "imports" "author_mapping" "none" => "Keep original authors as text";
    "imports" "author_mapping" "email" => "Match verified email";
    "imports" "author_mapping" "email_or_username" => "Match verified email or username";
    "provider" "kind" "oidc" => "OpenID Connect";
    "provider" "kind" "github" => "GitHub / GitHub Enterprise";
    "provider" "allow_registration" "true" => "Follow instance registration policy";
    "provider" "allow_registration" "false" => "Existing accounts only";
    "provider" "allow_registration" "always" => "Allow accounts even when registration is closed";
}

/// Text of the configuration schema, looked up by its keys. A key without an entry
/// renders as itself under the id `admin.configuration.unknown`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigText<'a> {
    Label { section: &'a str, key: &'a str },
    Help { section: &'a str, key: &'a str },
    Choice { section: &'a str, key: &'a str, value: &'a str },
    SectionTitle(&'a str),
    SectionHelp(&'a str),
}

const UNKNOWN: &str = "admin.configuration.unknown";

impl ConfigText<'static> {
    /// Every message of the schema, for catalog checks.
    pub fn all() -> Vec<Self> {
        let fields = fields_a::KEYS.iter().chain(fields_b::KEYS);
        let fields = fields.flat_map(|&(section, key)| [Self::Label { section, key }, Self::Help { section, key }]);
        let sections = SECTIONS.iter().flat_map(|&key| [Self::SectionTitle(key), Self::SectionHelp(key)]);
        let choices = CHOICES.iter().map(|&(section, key, value)| Self::Choice { section, key, value });
        fields.chain(sections).chain(choices).collect()
    }
}

impl<'a> ConfigText<'a> {
    /// Whether the catalog has an entry for this key.
    pub fn exists(self) -> bool {
        self.id() != UNKNOWN
    }

    /// The id and English text, or `None` for an unknown key.
    fn entry(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Label { section, key } => field(section, key).map(|field| (field.label_id, field.label)),
            Self::Help { section, key } => field(section, key).map(|field| (field.help_id, field.help)),
            Self::Choice { section, key, value } => choice(section, key, value).map(|[id, label]| (id, label)),
            Self::SectionTitle(key) => section(key).map(|[id, title, ..]| (id, title)),
            Self::SectionHelp(key) => section(key).map(|[.., id, help]| (id, help)),
        }
    }

    /// The key itself, shown for an unknown key.
    const fn key(self) -> &'a str {
        match self {
            Self::Label { key, .. } | Self::Help { key, .. } | Self::SectionTitle(key) | Self::SectionHelp(key) => key,
            Self::Choice { value, .. } => value,
        }
    }
}

localizes!(ConfigText<'a>);

impl MessageFamily for ConfigText<'_> {
    fn id(self) -> &'static str {
        self.entry().map_or(UNKNOWN, |(id, _)| id)
    }

    fn english(self, text: &mut MessageWriter) {
        match self.entry() {
            Some((_, english)) => text.literal(english),
            None => text.argument(self.key()),
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

labels! {
    /// Fixed labels of `/admin/configuration`.
    ConfigMessage, "admin.configuration" {
        Title = "title" => "Configuration",
        Intro = "intro" => "Manage your self-hosted instance with settings organized by feature.",
        RestartServer = "restart-server" => "Restart server",
        Saved = "saved" => "Settings saved. Restart the server to apply deployment changes.",
        RestartRequired = "restart-required" => "Restart required",
        EnvironmentWins = "environment-wins" => "Environment variables take precedence.",
        LivePolicies = "live-policies" => "Branding and policies — apply immediately",
        SettingsSection = "settings-section" => "Settings section",
        OpenSection = "open-section" => "Open section",
        Sections = "sections" => "Configuration sections",
        LiveOverrides = "live-overrides" => "Saved live branding and registration policies override these deployment defaults.",
        IdentityProviders = "identity-providers" => "Identity providers",
        ProvidersHint = "providers-hint" => "Add and remove provider cards first, then enter new credentials and save. Provider changes stay in this form until you save the section.",
        CallbackLead = "callback-lead" => "Register a callback URL with each provider: your Public URL followed by",
        CallbackTail = "callback-tail" => ", replacing PROVIDER_ID with the ID below. For OpenID Connect, set the issuer URL; for GitHub Enterprise, set the Enterprise URL.",
        NoProviders = "no-providers" => "No external sign-in providers configured. Local sign-in is available.",
        RemoveProvider = "remove-provider" => "Remove provider",
        AddProvider = "add-provider" => "Add identity provider",
        NewProvider = "new-provider" => "New sign-in provider",
        OnlySection = "only-section" => "Only this section will be updated.",
        Recovery = "recovery" => "Recovery and deployment defaults",
        RecoveryLead = "recovery-lead" => "If saved settings prevent startup, set",
        RecoveryTail = "recovery-tail" => "in the server environment, restart, then correct or remove the overrides here.",
        RemovingOverrides = "removing-overrides" => "Removing overrides restores file and environment configuration on the next restart. Live branding and policies are retained.",
        RemoveOverrides = "remove-overrides" => "Remove all deployment overrides",
        Optional = "field.optional" => "(optional)",
        OnePerLine = "field.one-per-line" => "One entry per line.",
        ProviderDefaults = "field.provider-defaults" => "Use provider defaults",
        Configured = "field.configured" => "Configured — leave blank to keep",
        NotConfigured = "field.not-configured" => "Not configured",
        SecretHint = "field.secret-hint" => "Leave blank to keep the current credential. New credentials are never redisplayed after an error; enter them again when correcting the form. Protect the instance database and its backups.",
        ClearSecret = "field.clear-secret" => "Clear this credential",
        BlankDefault = "field.blank-default" => "Leave blank to use the default.",
    }
}

messages! {
    /// Configuration page text with arguments.
    ConfigPageText<'a>, "admin.configuration" {
        SaveSection(section: &'a str) = "save-section" => ["Save ", {section}, " settings"],
    }
}
