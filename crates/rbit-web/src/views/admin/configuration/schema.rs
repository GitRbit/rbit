//! Field definitions for the administrator configuration forms. Their labels, help and
//! choice names live in the catalog (`rbit_localization::settings::config`), by key.
mod security;

use super::{Kind, Section, Spec};
use security::SECURITY;

const UPDATES: &[Spec] = &[
    Spec { key: "check_for_updates", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "report_version", kind: Kind::Toggle, optional: false, max: "" },
];

const SERVER: &[Spec] = &[
    Spec { key: "bind", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "public_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "site_name", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "open_registration", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "explore_enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "require_sign_in", kind: Kind::Toggle, optional: false, max: "" },
];

const STORAGE: &[Spec] = &[
    Spec { key: "data_dir", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "secret_key_file", kind: Kind::Text, optional: true, max: "" },
];

const REPOSITORY: &[Spec] = &[
    Spec { key: "default_branch", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "git_binary", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "max_render_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_git_processes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_git_requests_per_ip", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "git_idle_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "git_max_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_push_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_archives_per_ip", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "archive_idle_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "archive_max_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const MAIL: &[Spec] = &[
    Spec { key: "transport", kind: Kind::Choice(MAIL_TRANSPORT), optional: false, max: "" },
    Spec { key: "from_address", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "from_name", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "smtp_host", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "smtp_port", kind: Kind::Number, optional: true, max: "65535" },
    Spec { key: "smtp_security", kind: Kind::Choice(SMTP_SECURITY), optional: false, max: "" },
    Spec { key: "smtp_username", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "smtp_password", kind: Kind::Secret, optional: false, max: "" },
    Spec { key: "timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const SSH: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "bind", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "public_host", kind: Kind::Text, optional: true, max: "" },
    Spec { key: "public_port", kind: Kind::Number, optional: true, max: "65535" },
    Spec { key: "host_key_path", kind: Kind::Text, optional: true, max: "" },
    Spec { key: "max_connections", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_connections_per_ip", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "auth_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "idle_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_session_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const AUTH: &[Spec] = &[
    Spec { key: "timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "allow_private_addresses", kind: Kind::Toggle, optional: false, max: "" },
];

const BRANDING: &[Spec] = &[
    Spec { key: "primary_color", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "primary_text_color", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "description", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "welcome_title", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "welcome_message", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "logo_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "favicon_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "app_icon_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "footer_text", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "support_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "terms_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "privacy_url", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "show_powered_by", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "show_version", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "show_features", kind: Kind::Toggle, optional: false, max: "" },
];

const NOTIFICATIONS: &[Spec] = &[
    Spec { key: "email", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "default_email", kind: Kind::Choice(EMAIL_PREFERENCE), optional: false, max: "" },
    Spec { key: "digest_hour", kind: Kind::Number, optional: false, max: "23" },
];

const OPERATIONS: &[Spec] = &[
    Spec { key: "metrics", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "metrics_token", kind: Kind::Secret, optional: true, max: "" },
    Spec { key: "warm_caches_on_push", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "maintenance_interval_minutes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "job_concurrency", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "insights_cache_max_age_days", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "insights_cache_max_entries", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const API: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "default_page_size", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "max_page_size", kind: Kind::Number, optional: false, max: "4294967295" },
];

const WEBHOOKS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "allow_private_addresses", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_attempts", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "concurrency", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "delivery_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
];

const RELEASES: &[Spec] = &[
    Spec { key: "max_asset_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_assets_per_release", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "upload_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const MIRRORS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "default_interval_minutes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "min_interval_minutes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "fetch_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "allow_private_addresses", kind: Kind::Toggle, optional: false, max: "" },
];

const PUSH_MIRRORS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "interval_minutes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "debounce_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "push_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "allow_private_addresses", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "allow_local_urls", kind: Kind::Toggle, optional: false, max: "" },
];

const IMPORTS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "allow_private_addresses", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "max_concurrent", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "git_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "api_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "rate_limit_max_wait_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "author_mapping", kind: Kind::Choice(AUTHOR_MAPPING), optional: false, max: "" },
];

const ISSUES: &[Spec] = &[
    Spec { key: "scan_commit_limit", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "notifications", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "notification_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
];

const PULLS: &[Spec] = &[
    Spec { key: "merge_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_concurrent_merges", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "mergeability_concurrency", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_commits", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "noreply_domain", kind: Kind::Text, optional: false, max: "" },
];

const EDITOR: &[Spec] = &[
    Spec { key: "max_upload_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_upload_files", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "upload_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

const LFS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "max_object_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "token_lifetime_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
];

// Actions: foundation (F).
const ACTIONS: &[Spec] = &[
    Spec { key: "enabled", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "enable_for_new_repositories", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "secrets_key_file", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "log_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "run_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "max_log_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_workflow_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "zombie_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "endless_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "abandoned_timeout_seconds", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "evaluation_concurrency", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "instance_runners_for_all", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "max_log_storage_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_waiting_jobs_per_repository", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "max_runs_per_event", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "max_jobs_per_event", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "max_schedules_per_repository", kind: Kind::Number, optional: false, max: "4294967295" },
    // Actions: S.
    Spec { key: "long_poll_seconds", kind: Kind::Number, optional: false, max: "25" },
    Spec { key: "max_polls", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    // Actions: artifacts (A).
    Spec { key: "max_artifact_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_artifact_storage_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "artifact_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
    // Actions: shared cache.
    Spec { key: "max_cache_storage_bytes", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "cache_retention_days", kind: Kind::Number, optional: false, max: "4294967295" },
];

pub(super) const PROVIDER: &[Spec] = &[
    Spec { key: "kind", kind: Kind::Choice(PROVIDER_KIND), optional: false, max: "" },
    Spec { key: "id", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "name", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "issuer", kind: Kind::Text, optional: true, max: "" },
    Spec { key: "base_url", kind: Kind::Text, optional: true, max: "" },
    Spec { key: "client_id", kind: Kind::Text, optional: false, max: "" },
    Spec { key: "client_secret", kind: Kind::Secret, optional: false, max: "" },
    Spec { key: "scopes", kind: Kind::Lines, optional: true, max: "" },
    Spec { key: "allow_registration", kind: Kind::Choice(PROVIDER_REGISTRATION), optional: false, max: "" },
    Spec { key: "trust_email", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "trust_mfa", kind: Kind::Toggle, optional: false, max: "" },
];

pub(super) const SECTIONS: &[Section] = &[
    Section { key: "updates", fields: UPDATES },
    Section { key: "server", fields: SERVER },
    Section { key: "storage", fields: STORAGE },
    Section { key: "security", fields: SECURITY },
    Section { key: "repository", fields: REPOSITORY },
    Section { key: "mail", fields: MAIL },
    Section { key: "ssh", fields: SSH },
    Section { key: "auth", fields: AUTH },
    Section { key: "branding", fields: BRANDING },
    Section { key: "notifications", fields: NOTIFICATIONS },
    Section { key: "operations", fields: OPERATIONS },
    Section { key: "api", fields: API },
    Section { key: "webhooks", fields: WEBHOOKS },
    Section { key: "releases", fields: RELEASES },
    Section { key: "mirrors", fields: MIRRORS },
    Section { key: "push_mirrors", fields: PUSH_MIRRORS },
    Section { key: "imports", fields: IMPORTS },
    Section { key: "issues", fields: ISSUES },
    Section { key: "pulls", fields: PULLS },
    Section { key: "editor", fields: EDITOR },
    Section { key: "lfs", fields: LFS },
    // Actions: foundation (F).
    Section { key: "actions", fields: ACTIONS },
];

const MAIL_TRANSPORT: &[&str] = &["disabled", "log", "smtp"];
const SMTP_SECURITY: &[&str] = &["starttls", "tls", "none"];
const EMAIL_PREFERENCE: &[&str] = &["off", "participating", "all"];
const AUTHOR_MAPPING: &[&str] = &["none", "email", "email_or_username"];
const PROVIDER_KIND: &[&str] = &["oidc", "github"];
const PROVIDER_REGISTRATION: &[&str] = &["true", "false", "always"];

#[cfg(test)]
mod tests {
    use rbit_localization::settings::config::ConfigText;

    use super::{Kind, PROVIDER, SECTIONS};

    #[test]
    fn every_section_field_and_choice_has_catalog_text() {
        let groups = SECTIONS.iter().map(|section| (section.key, section.fields)).chain([("provider", PROVIDER)]);
        for (section, fields) in groups {
            assert!(section == "provider" || ConfigText::SectionTitle(section).exists() && ConfigText::SectionHelp(section).exists());
            for spec in fields {
                let key = spec.key;
                assert!(ConfigText::Label { section, key }.exists() && ConfigText::Help { section, key }.exists(), "{section}.{key}");
                if let Kind::Choice(values) = spec.kind {
                    for &value in values {
                        assert!(ConfigText::Choice { section, key, value }.exists(), "{section}.{key}.{value}");
                    }
                }
            }
        }
    }
}
