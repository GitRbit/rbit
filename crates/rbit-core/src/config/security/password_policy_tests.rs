//! Password admission defaults remain backward compatible while deployment bounds fail closed.

use super::SecurityConfig;

#[test]
fn standalone_defaults_and_old_saved_documents_supply_two_password_jobs() {
    assert_eq!(SecurityConfig::default().password_job_capacity().unwrap().get(), 2);
    let old: SecurityConfig = serde_json::from_str(r#"{"min_password_length":12,"rate_limit":false}"#).unwrap();
    assert_eq!(old.max_password_jobs, 2);
    assert!(!old.rate_limit);
    let serialized = serde_json::to_value(&old).unwrap();
    assert_eq!(serialized["max_password_jobs"], 2);
    let restored: SecurityConfig = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.password_job_capacity().unwrap().get(), 2);
}

#[test]
fn password_job_capacity_accepts_boundaries_and_rejects_zero_or_excessive_values() {
    for capacity in [1, 2, 64] {
        let config = SecurityConfig { max_password_jobs: capacity, ..SecurityConfig::default() };
        assert_eq!(config.password_job_capacity().unwrap().get(), capacity);
    }
    for capacity in [0, 65, usize::MAX] {
        let config = SecurityConfig { max_password_jobs: capacity, ..SecurityConfig::default() };
        assert_eq!(config.password_job_capacity().unwrap_err(), "security.max_password_jobs must be between 1 and 64");
    }
    for json in [r#"{"max_password_jobs":-1}"#, r#"{"max_password_jobs":1.5}"#, r#"{"max_password_jobs":"2"}"#] {
        assert!(serde_json::from_str::<SecurityConfig>(json).is_err());
    }
}
