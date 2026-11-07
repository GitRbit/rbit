//! Security controls and restart-required password-work admission policy.

use super::{Kind, Spec};

pub(super) const SECURITY: &[Spec] = &[
    Spec {
        key: "session_lifetime_days",
        label: "Session lifetime (days)",
        help: "How long a session stays valid without activity, in days.",
        kind: Kind::Number,
        optional: false,
        max: "4294967295",
    },
    Spec {
        key: "session_max_age_days",
        label: "Maximum session age (days)",
        help: "How long a session stays valid at most, however active, in days. 0 means no limit.",
        kind: Kind::Number,
        optional: false,
        max: "4294967295",
    },
    Spec {
        key: "min_password_length",
        label: "Min password length",
        help: "Minimum password length accepted at sign-up.",
        kind: Kind::Number,
        optional: false,
        max: "18446744073709551615",
    },
    Spec {
        key: "max_password_jobs",
        label: "Maximum password jobs",
        help: "Simultaneous hashing or verification jobs in the shared password-work budget (1–64; default 2). Restart required.",
        kind: Kind::Number,
        optional: false,
        max: "64",
    },
    Spec {
        key: "trusted_proxies",
        label: "Trusted proxy networks",
        help: "Reverse proxies (CIDR networks, e.g. 127.0.0.1/32, 10.0.0.0/8, ::1/128) whose X-Forwarded-For, Forwarded and X-Forwarded-Proto headers are believed. Empty (the default) means the TCP peer is always the client.",
        kind: Kind::Lines,
        optional: false,
        max: "",
    },
    Spec {
        key: "rate_limit",
        label: "Enable rate limiting",
        help: "Throttle sign-in, sign-up, two-factor codes, Git credentials and form posts per client. Only worth turning off for load tests.",
        kind: Kind::Toggle,
        optional: false,
        max: "",
    },
    Spec {
        key: "hsts",
        label: "Send HSTS",
        help: "Send Strict-Transport-Security (one year) when the public URL is https://, so browsers refuse plain HTTP to this host. Turn off only while moving away from HTTPS.",
        kind: Kind::Toggle,
        optional: false,
        max: "",
    },
    Spec {
        key: "hsts_include_subdomains",
        label: "HSTS includes subdomains",
        help: "Add includeSubDomains to that header, binding every subdomain of the host to HTTPS as well.",
        kind: Kind::Toggle,
        optional: false,
        max: "",
    },
    Spec {
        key: "external_images",
        label: "Allow external images",
        help: "Let pages show images from other sites, which also tells those sites who viewed the page. Off limits images to this instance and inline data: images.",
        kind: Kind::Toggle,
        optional: false,
        max: "",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rbit_core::Config;

    use crate::views::admin::configuration::{ConfigurationForm, apply_submission};

    #[test]
    fn password_capacity_has_a_numeric_control_and_restart_guidance_and_round_trips() {
        let mut document = serde_json::to_value(Config::default()).unwrap();
        let form = ConfigurationForm::new(&document, "security").unwrap();
        let field = form.fields.iter().find(|field| field.name == "max_password_jobs").unwrap();
        assert_eq!((field.kind, field.value.as_str(), field.max), ("number", "2", "64"));
        assert!(field.help.contains("1–64") && field.help.contains("Restart required"));
        let mut inputs: BTreeMap<_, _> = form
            .fields
            .iter()
            .filter(|field| field.kind != "checkbox" || field.value == "true")
            .map(|field| (field.name.clone(), field.value.clone()))
            .collect();
        inputs.insert("max_password_jobs".into(), "64".into());
        assert!(apply_submission(&mut document, "security", &inputs).unwrap());
        let restored: Config = serde_json::from_value(document.clone()).unwrap();
        assert_eq!(restored.security.password_job_capacity().unwrap().get(), 64);
        inputs.insert("max_password_jobs".into(), "65".into());
        assert!(apply_submission(&mut document, "security", &inputs).is_err());
    }
}
