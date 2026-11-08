//! Security controls and restart-required password-work admission policy.

use super::{Kind, Spec};

pub(super) const SECURITY: &[Spec] = &[
    Spec { key: "session_lifetime_days", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "session_max_age_days", kind: Kind::Number, optional: false, max: "4294967295" },
    Spec { key: "min_password_length", kind: Kind::Number, optional: false, max: "18446744073709551615" },
    Spec { key: "max_password_jobs", kind: Kind::Number, optional: false, max: "64" },
    Spec { key: "trusted_proxies", kind: Kind::Lines, optional: false, max: "" },
    Spec { key: "rate_limit", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "hsts", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "hsts_include_subdomains", kind: Kind::Toggle, optional: false, max: "" },
    Spec { key: "external_images", kind: Kind::Toggle, optional: false, max: "" },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rbit_core::Config;

    use crate::views::admin::configuration::{ConfigurationForm, apply_submission};

    #[test]
    fn password_capacity_has_a_numeric_control_and_restart_guidance_and_round_trips() {
        let mut document = serde_json::to_value(Config::default()).unwrap();
        let form = ConfigurationForm::new(&document, "security", rbit_localization::Locale::English).unwrap();
        let field = form.fields.iter().find(|field| field.name == "max_password_jobs").unwrap();
        assert_eq!((field.kind, field.value.as_str(), field.max), ("number", "2", "64"));
        assert!(field.help.as_str().contains("1–64") && field.help.as_str().contains("Restart required"));
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
