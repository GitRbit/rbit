//! Passive maintenance comparisons, never executable authorization or renewal.

use super::{HostPolicyFingerprint, POLICY_SCHEMA_VERSION, PolicyError};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};
use std::fmt;

/// Host input and compact canonical JSON bound for a passive renewal request.
pub const MAX_POLICY_RENEWAL_REQUEST_BYTES: usize = 1024;

/// Passive request to retire the exact observed sealed policy epoch.
///
/// This is comparison metadata, never permission to reopen a gate, delete audit,
/// create an executable handle or choose a new epoch. The trusted host must check
/// live administrator rights, observed seal/fingerprint and retention capacity in
/// its owned atomic transaction. It chooses a fresh epoch, preserves retired audit
/// and a caller-known idempotency receipt, and carries no review/selection forward.
/// Hosts must cap COMPLETE serialized input at [`MAX_POLICY_RENEWAL_REQUEST_BYTES`]
/// before deserialization, and revalidate mutable fields before canonical storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackagePolicyRenewalRequestWire")]
pub struct PackagePolicyRenewalRequest {
    pub schema_version: u32,
    pub expected_host_policy: HostPolicyFingerprint,
    /// Observed seal key only; distinct from this renewal's own idempotency key.
    pub expected_seal_operation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackagePolicyRenewalRequestWire {
    schema_version: u32,
    expected_host_policy: HostPolicyFingerprint,
    #[serde(deserialize_with = "seal_operation")]
    expected_seal_operation: String,
}
fn valid_seal_operation(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn seal_operation<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct SealOperationVisitor;
    impl Visitor<'_> for SealOperationVisitor {
        type Value = String;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("1-64 lowercase ASCII letters, digits or hyphens")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            if !valid_seal_operation(value) {
                return Err(E::custom(PolicyError::SealOperation));
            }
            Ok(value.to_owned())
        }
        fn visit_string<E: de::Error>(self, value: String) -> Result<String, E> {
            if !valid_seal_operation(&value) {
                return Err(E::custom(PolicyError::SealOperation));
            }
            Ok(value)
        }
    }
    deserializer.deserialize_string(SealOperationVisitor)
}
impl PackagePolicyRenewalRequest {
    pub fn new(expected_host_policy: HostPolicyFingerprint, expected_seal_operation: impl Into<String>) -> Result<Self, PolicyError> {
        let request =
            Self { schema_version: POLICY_SCHEMA_VERSION, expected_host_policy, expected_seal_operation: expected_seal_operation.into() };
        request.validate()?;
        Ok(request)
    }
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema_version != POLICY_SCHEMA_VERSION {
            return Err(PolicyError::Schema);
        }
        if !valid_seal_operation(&self.expected_seal_operation) {
            return Err(PolicyError::SealOperation);
        }
        // Every value is validated ASCII requiring no JSON escapes, schema is1,
        // and Serialize emits the declaration-order compact three-field shape.
        // Check its exact bound without adding a JSON runtime dependency.
        let syntax_bytes = r#"{"schema_version":1,"expected_host_policy":"","expected_seal_operation":""}"#.len();
        let bytes = syntax_bytes
            .checked_add(self.expected_host_policy.as_str().len())
            .and_then(|n| n.checked_add(self.expected_seal_operation.len()))
            .ok_or(PolicyError::RenewalRequestSize)?;
        if bytes > MAX_POLICY_RENEWAL_REQUEST_BYTES {
            return Err(PolicyError::RenewalRequestSize);
        }
        Ok(())
    }
}
impl TryFrom<PackagePolicyRenewalRequestWire> for PackagePolicyRenewalRequest {
    type Error = PolicyError;
    fn try_from(value: PackagePolicyRenewalRequestWire) -> Result<Self, Self::Error> {
        let request = Self {
            schema_version: value.schema_version,
            expected_host_policy: value.expected_host_policy,
            expected_seal_operation: value.expected_seal_operation,
        };
        request.validate()?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renewal_compact_json_is_deterministic_bounded_and_passive() {
        let fingerprint = HostPolicyFingerprint::new("b".repeat(64).parse().unwrap());
        let request = PackagePolicyRenewalRequest::new(fingerprint, "observed-seal-12").unwrap();
        let actual = serde_json::to_string(&request).unwrap();
        let expected =
            format!(r#"{{"schema_version":1,"expected_host_policy":"{}","expected_seal_operation":"observed-seal-12"}}"#, "b".repeat(64));
        assert_eq!(actual, expected);
        assert_eq!(serde_json::from_str::<PackagePolicyRenewalRequest>(&actual).unwrap(), request);
        let maximum = PackagePolicyRenewalRequest::new(request.expected_host_policy.clone(), "a".repeat(64)).unwrap();
        assert!(serde_json::to_vec(&maximum).unwrap().len() <= MAX_POLICY_RENEWAL_REQUEST_BYTES);
        let reordered =
            format!(r#"{{"expected_seal_operation":"observed-seal-12","expected_host_policy":"{}","schema_version":1}}"#, "b".repeat(64));
        assert_eq!(serde_json::to_string(&serde_json::from_str::<PackagePolicyRenewalRequest>(&reordered).unwrap()).unwrap(), expected);
    }
    #[test]
    fn renewal_requires_strict_schema_all_comparisons_and_no_authority_fields() {
        let value = json!({"schema_version":1,"expected_host_policy":"b".repeat(64),"expected_seal_operation":"observed-seal"});
        for field in ["force", "grants", "new_epoch", "path", "execution", "selected_package"] {
            let mut invalid = value.clone();
            invalid[field] = json!(true);
            assert!(serde_json::from_value::<PackagePolicyRenewalRequest>(invalid).is_err());
        }
        for field in ["schema_version", "expected_host_policy", "expected_seal_operation"] {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PackagePolicyRenewalRequest>(invalid).is_err());
        }
        for schema in [json!(0), json!(2), json!("1")] {
            let mut invalid = value.clone();
            invalid["schema_version"] = schema;
            assert!(serde_json::from_value::<PackagePolicyRenewalRequest>(invalid).is_err());
        }
        let duplicate = format!(
            r#"{{"schema_version":1,"schema_version":1,"expected_host_policy":"{}","expected_seal_operation":"observed-seal"}}"#,
            "b".repeat(64)
        );
        assert!(serde_json::from_str::<PackagePolicyRenewalRequest>(&duplicate).is_err());
        let mut invalid = value;
        invalid["expected_host_policy"] = json!("B".repeat(64));
        assert!(serde_json::from_value::<PackagePolicyRenewalRequest>(invalid).is_err());
    }
    #[test]
    fn renewal_rejects_malformed_seal_keys_in_serde_and_mutable_values() {
        let fp = HostPolicyFingerprint::new("b".repeat(64).parse().unwrap());
        for key in ["", "UPPER", "../escape", "a/b", "a?b", "a b", "é", "a\0", "a\nb", &"a".repeat(65)] {
            assert_eq!(PackagePolicyRenewalRequest::new(fp.clone(), key), Err(PolicyError::SealOperation));
            assert!(
                serde_json::from_value::<PackagePolicyRenewalRequest>(
                    json!({"schema_version":1,"expected_host_policy":fp,"expected_seal_operation":key})
                )
                .is_err()
            );
        }
        let mut request = PackagePolicyRenewalRequest::new(fp, "-known-seal").unwrap();
        request.schema_version = 2;
        assert_eq!(request.validate(), Err(PolicyError::Schema));
        request.schema_version = 1;
        request.expected_seal_operation = "x".repeat(65);
        assert_eq!(request.validate(), Err(PolicyError::SealOperation));
        request.expected_seal_operation = "valid-1".into();
        assert_eq!(request.validate(), Ok(()));
    }
}
