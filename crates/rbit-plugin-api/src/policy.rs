//! Passive package review/selection requests, never executable authorization.
//!
//! Hosts must cap complete serialized input at [`MAX_POLICY_REQUEST_BYTES`] before
//! deserializing and revalidate programmatically modified requests. These types do
//! not prove installation, byte integrity, publisher identity, administrator rights,
//! current policy, resource ownership, available adapters or effective grants.
//! Unknown capabilities grant no calls; the trusted host must recognize and enforce
//! each operation. Native code with the service OS user's identity is fully trusted
//! OS-user code: broker grants cannot sandbox its filesystem, process or network use.
//!
//! A host must resolve the exact durable installation/digest, compare the current
//! review/selection revisions and host policy fingerprint atomically, and reject a
//! historical allow after revocation. Clearing/recreating state must not reuse a
//! revision (ABA). Capacity exhaustion must not prevent a deny/revocation while old
//! authority remains usable: reserve a fail-closed revocation path or close effective
//! execution/host-call admission, including existing generations. Merely rejecting
//! new review commands is insufficient. No policy execution/commit, hash computation or signature verification
//! is implemented here.

use std::{collections::BTreeSet, fmt, num::NonZeroU64, str::FromStr};

use semver::Version;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, IgnoredAny, SeqAccess, Visitor},
};

use crate::{CapabilityId, PackageKind, PackageManifest, PluginId, Sha256Digest};

pub const POLICY_SCHEMA_VERSION: u32 = 1;
pub const MAX_POLICY_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_POLICY_GRANTS: usize = 128;
const MAX_FULL_VERSION_BYTES: usize = 128;

/// Static contract failures; transports must also map serde errors to safe output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("unsupported policy request schema")]
    Schema,
    #[error("invalid bounded canonical full package version")]
    Version,
    #[error("expected canonical positive decimal value within the signed 64-bit domain")]
    Sequence,
    #[error("too many policy grant requests")]
    GrantCount,
    #[error("duplicate policy grant request")]
    DuplicateGrant,
    #[error("deny and data-only review intents cannot request broker grants")]
    GrantsWithoutExecution,
    #[error("review package does not match the supplied manifest identity")]
    ManifestIdentity,
    #[error("review intent does not match the declared package kind")]
    PackageKind,
    #[error("grant capability was not requested by the manifest")]
    UndeclaredCapability,
    #[error("supplied package manifest is invalid")]
    Manifest,
}

macro_rules! decimal_value {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU64);
        impl $name {
            pub const MAX: u64 = i64::MAX as u64;
            pub fn new(value: u64) -> Result<Self, PolicyError> {
                if value > Self::MAX {
                    return Err(PolicyError::Sequence);
                }
                NonZeroU64::new(value).map(Self).ok_or(PolicyError::Sequence)
            }
            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
        impl FromStr for $name {
            type Err = PolicyError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                if value.is_empty() || value.len() > 19 || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(PolicyError::Sequence);
                }
                Self::new(value.parse().map_err(|_| PolicyError::Sequence)?)
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.get().to_string())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Decimal;
                impl Visitor<'_> for Decimal {
                    type Value = $name;
                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("a canonical bounded positive decimal string")
                    }
                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        value.parse().map_err(E::custom)
                    }
                }
                deserializer.deserialize_str(Decimal)
            }
        }
    };
}
decimal_value!(
    PolicyRevision,
    "Nonzero revision, serialized as canonical decimal text to preserve exact client precision. Not a credential."
);
decimal_value!(
    RepositoryGrantId,
    "Passive repository subject ID; the host must bind a live FK-backed subject or resource incarnation, never authorize a reused numeric ID."
);

/// An explicit expectation, with no zero sentinel or unconditional-write variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpectedPolicyRevision {
    Absent {},
    Exact { revision: PolicyRevision },
}

/// Compare-only host policy identity. SHA syntax proves neither trust nor authority.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostPolicyFingerprint(Sha256Digest);
impl HostPolicyFingerprint {
    pub fn new(value: Sha256Digest) -> Self {
        Self(value)
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exact immutable archive identity. Full `SemVer`, including build metadata, is
/// compared for equality; dependency ranges/precedence must not select a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "InstalledPackageRefWire")]
pub struct InstalledPackageRef {
    plugin_id: PluginId,
    version: Version,
    archive_digest: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledPackageRefWire {
    plugin_id: PluginId,
    #[serde(deserialize_with = "full_version")]
    version: Version,
    archive_digest: Sha256Digest,
}
impl InstalledPackageRef {
    pub fn new(plugin_id: PluginId, version: Version, archive_digest: Sha256Digest) -> Result<Self, PolicyError> {
        if version.to_string().len() > MAX_FULL_VERSION_BYTES {
            return Err(PolicyError::Version);
        }
        Ok(Self { plugin_id, version, archive_digest })
    }
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
    pub fn archive_digest(&self) -> &Sha256Digest {
        &self.archive_digest
    }
}
impl TryFrom<InstalledPackageRefWire> for InstalledPackageRef {
    type Error = PolicyError;
    fn try_from(value: InstalledPackageRefWire) -> Result<Self, Self::Error> {
        Self::new(value.plugin_id, value.version, value.archive_digest)
    }
}
fn full_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Version, D::Error> {
    struct FullVersion;
    impl Visitor<'_> for FullVersion {
        type Value = Version;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded canonical full semantic version")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if value.len() > MAX_FULL_VERSION_BYTES {
                return Err(E::custom(PolicyError::Version));
            }
            let version = Version::parse(value).map_err(|_| E::custom(PolicyError::Version))?;
            if version.to_string() != value {
                return Err(E::custom(PolicyError::Version));
            }
            Ok(version)
        }
    }
    deserializer.deserialize_str(FullVersion)
}

/// Administrative review intent only. Wasm requires a real sandbox adapter and
/// must never fall back to native execution. Native approval acknowledges full
/// service-OS-user trust, not an OS restriction inferred from broker grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageReviewIntent {
    Deny {},
    DataOnly {},
    NativeFullServiceUserTrust {},
    WasmSandboxRequired {},
}

/// The host catalog defines valid capability/scope combinations. Instance is not
/// an implicit wildcard for every repository, nor a substitute for current user,
/// workload and repository policy. A repository ID alone is not durable authority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicyGrantScope {
    Instance {},
    Repository { repository_id: RepositoryGrantId },
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyGrantRequest {
    pub capability: CapabilityId,
    pub scope: PolicyGrantScope,
}

/// Canonically sorted bounded requests, never effective grants. Duplicate tuples
/// are rejected rather than silently deduplicated; input order does not affect
/// serialized replay identity. No default grant set or inherited/merge semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyGrantRequests(#[serde(deserialize_with = "grant_requests")] Vec<PolicyGrantRequest>);
impl PolicyGrantRequests {
    pub fn new(mut requests: Vec<PolicyGrantRequest>) -> Result<Self, PolicyError> {
        if requests.len() > MAX_POLICY_GRANTS {
            return Err(PolicyError::GrantCount);
        }
        requests.sort();
        if requests.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(PolicyError::DuplicateGrant);
        }
        Ok(Self(requests))
    }
    pub fn as_slice(&self) -> &[PolicyGrantRequest] {
        &self.0
    }
}
fn grant_requests<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PolicyGrantRequest>, D::Error> {
    struct Grants;
    impl<'de> Visitor<'de> for Grants {
        type Value = Vec<PolicyGrantRequest>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded array of unique explicit grant requests")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut grants = Vec::new();
            while grants.len() < MAX_POLICY_GRANTS {
                let Some(grant) = sequence.next_element()? else {
                    return PolicyGrantRequests::new(grants).map(|value| value.0).map_err(de::Error::custom);
                };
                grants.push(grant);
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(PolicyError::GrantCount));
            }
            PolicyGrantRequests::new(grants).map(|value| value.0).map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_seq(Grants)
}

/// A complete proposed replacement decision; the trusted host must authorize it.
/// Private grant-list invariants survive programmatic construction, but mutable
/// schema/intent fields still require `validate()` at the host boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackageReviewRequestWire")]
pub struct PackageReviewRequest {
    pub schema_version: u32,
    pub package: InstalledPackageRef,
    pub expected_review: ExpectedPolicyRevision,
    pub expected_host_policy: HostPolicyFingerprint,
    pub intent: PackageReviewIntent,
    pub grants: PolicyGrantRequests,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageReviewRequestWire {
    schema_version: u32,
    package: InstalledPackageRef,
    expected_review: ExpectedPolicyRevision,
    expected_host_policy: HostPolicyFingerprint,
    intent: PackageReviewIntent,
    grants: PolicyGrantRequests,
}
impl PackageReviewRequest {
    pub fn new(
        package: InstalledPackageRef,
        expected_review: ExpectedPolicyRevision,
        expected_host_policy: HostPolicyFingerprint,
        intent: PackageReviewIntent,
        grants: PolicyGrantRequests,
    ) -> Result<Self, PolicyError> {
        let request = Self { schema_version: POLICY_SCHEMA_VERSION, package, expected_review, expected_host_policy, intent, grants };
        request.validate()?;
        Ok(request)
    }
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema_version != POLICY_SCHEMA_VERSION {
            return Err(PolicyError::Schema);
        }
        if matches!(self.intent, PackageReviewIntent::Deny {} | PackageReviewIntent::DataOnly {}) && !self.grants.as_slice().is_empty() {
            return Err(PolicyError::GrantsWithoutExecution);
        }
        Ok(())
    }
    /// Structural declaration checks only. The caller must supply the manifest of
    /// this exact durable archive; matching metadata does not prove that linkage.
    /// Current approvals, recognized catalog, subject incarnation, administrator
    /// authority, policy intersection and adapter availability remain host checks.
    pub fn validate_manifest_declarations(&self, manifest: &PackageManifest) -> Result<(), PolicyError> {
        self.validate()?;
        manifest.validate().map_err(|_| PolicyError::Manifest)?;
        if self.package.plugin_id() != &manifest.plugin.id || self.package.version() != &manifest.plugin.version {
            return Err(PolicyError::ManifestIdentity);
        }
        let matches_kind = matches!(
            (&self.intent, &manifest.kind),
            (PackageReviewIntent::Deny {}, _)
                | (PackageReviewIntent::DataOnly {}, PackageKind::Data {})
                | (PackageReviewIntent::NativeFullServiceUserTrust {}, PackageKind::Native { .. })
                | (PackageReviewIntent::WasmSandboxRequired {}, PackageKind::Wasm { .. })
        );
        if !matches_kind {
            return Err(PolicyError::PackageKind);
        }
        let requested: BTreeSet<_> = manifest.plugin.capabilities.iter().collect();
        if self.grants.as_slice().iter().any(|grant| !requested.contains(&grant.capability)) {
            return Err(PolicyError::UndeclaredCapability);
        }
        Ok(())
    }
}
impl TryFrom<PackageReviewRequestWire> for PackageReviewRequest {
    type Error = PolicyError;
    fn try_from(value: PackageReviewRequestWire) -> Result<Self, Self::Error> {
        let request = Self {
            schema_version: value.schema_version,
            package: value.package,
            expected_review: value.expected_review,
            expected_host_policy: value.expected_host_policy,
            intent: value.intent,
            grants: value.grants,
        };
        request.validate()?;
        Ok(request)
    }
}

/// Passive candidate selection. The named review revision must still be the
/// current allowed decision under the expected host policy when the host commits;
/// these fields do not establish that fact or activate an embedded/managed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackageSelectionRequestWire")]
pub struct PackageSelectionRequest {
    pub schema_version: u32,
    pub package: InstalledPackageRef,
    pub expected_selection: ExpectedPolicyRevision,
    pub expected_review_revision: PolicyRevision,
    pub expected_host_policy: HostPolicyFingerprint,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageSelectionRequestWire {
    schema_version: u32,
    package: InstalledPackageRef,
    expected_selection: ExpectedPolicyRevision,
    expected_review_revision: PolicyRevision,
    expected_host_policy: HostPolicyFingerprint,
}
impl PackageSelectionRequest {
    pub fn new(
        package: InstalledPackageRef,
        expected_selection: ExpectedPolicyRevision,
        expected_review_revision: PolicyRevision,
        expected_host_policy: HostPolicyFingerprint,
    ) -> Self {
        Self { schema_version: POLICY_SCHEMA_VERSION, package, expected_selection, expected_review_revision, expected_host_policy }
    }
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema_version != POLICY_SCHEMA_VERSION {
            return Err(PolicyError::Schema);
        }
        Ok(())
    }
}
impl TryFrom<PackageSelectionRequestWire> for PackageSelectionRequest {
    type Error = PolicyError;
    fn try_from(value: PackageSelectionRequestWire) -> Result<Self, Self::Error> {
        let request = Self {
            schema_version: value.schema_version,
            package: value.package,
            expected_selection: value.expected_selection,
            expected_review_revision: value.expected_review_revision,
            expected_host_policy: value.expected_host_policy,
        };
        request.validate()?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn reference() -> InstalledPackageRef {
        InstalledPackageRef::new(
            "example.greeter".parse().unwrap(),
            Version::parse("1.0.0+build.a").unwrap(),
            "a".repeat(64).parse().unwrap(),
        )
        .unwrap()
    }
    fn fingerprint() -> HostPolicyFingerprint {
        HostPolicyFingerprint::new("b".repeat(64).parse().unwrap())
    }
    fn grant(repository: u64) -> PolicyGrantRequest {
        PolicyGrantRequest {
            capability: "repository.read".parse().unwrap(),
            scope: PolicyGrantScope::Repository { repository_id: RepositoryGrantId::new(repository).unwrap() },
        }
    }
    fn request() -> PackageReviewRequest {
        PackageReviewRequest::new(
            reference(),
            ExpectedPolicyRevision::Absent {},
            fingerprint(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(vec![grant(7)]).unwrap(),
        )
        .unwrap()
    }
    fn manifest() -> PackageManifest {
        serde_json::from_value(json!({
            "schema_version": 1,
            "plugin": {"id":"example.greeter", "version":"1.0.0+build.a", "host_api":"^1", "capabilities":["repository.read"]},
            "kind": {"type":"native", "target":"x86_64-unknown-linux-gnu", "entrypoint":"greeter"},
            "files": [{"path":"greeter", "size":1, "sha256":"c".repeat(64)}],
            "budgets": {"memory_bytes":1024, "concurrent_calls":1, "call_timeout_ms":1000, "storage_bytes":0, "event_queue_capacity":0}
        }))
        .unwrap()
    }

    #[test]
    fn decimal_values_are_exact_and_canonical_at_all_boundaries() {
        for value in [1, 9_007_199_254_740_993, PolicyRevision::MAX] {
            let revision = PolicyRevision::new(value).unwrap();
            let wire = format!("\"{value}\"");
            assert_eq!(serde_json::to_string(&revision).unwrap(), wire);
            assert_eq!(serde_json::from_str::<PolicyRevision>(&wire).unwrap(), revision);
            assert_eq!(serde_json::from_str::<RepositoryGrantId>(&wire).unwrap().get(), value);
        }
        for wire in [
            "0",
            "1",
            "null",
            "true",
            "\"\"",
            "\"0\"",
            "\"01\"",
            "\"+1\"",
            "\"-1\"",
            "\"1.0\"",
            "\"1e0\"",
            "\" 1\"",
            "\"1 \"",
            "\"9223372036854775808\"",
            "\"18446744073709551616\"",
        ] {
            assert!(serde_json::from_str::<PolicyRevision>(wire).is_err(), "{wire}");
            assert!(serde_json::from_str::<RepositoryGrantId>(wire).is_err(), "{wire}");
        }
        assert_eq!(PolicyRevision::new(0), Err(PolicyError::Sequence));
        assert_eq!(RepositoryGrantId::new(u64::MAX), Err(PolicyError::Sequence));
        assert_eq!(
            serde_json::from_value::<ExpectedPolicyRevision>(json!({"kind":"exact", "revision":"1"})).unwrap(),
            ExpectedPolicyRevision::Exact { revision: PolicyRevision::new(1).unwrap() }
        );
        for value in [json!({"kind":"absent", "revision":"1"}), json!({"kind":"exact"}), json!({"kind":"any"})] {
            assert!(serde_json::from_value::<ExpectedPolicyRevision>(value).is_err());
        }
    }

    #[test]
    fn exact_identity_preserves_build_metadata_and_archive_digest() {
        let first = reference();
        let second =
            InstalledPackageRef::new(first.plugin_id().clone(), Version::parse("1.0.0+build.b").unwrap(), first.archive_digest().clone())
                .unwrap();
        assert_eq!(first.version().cmp_precedence(second.version()), std::cmp::Ordering::Equal);
        assert_ne!(first, second);
        assert_ne!(
            first,
            InstalledPackageRef::new(first.plugin_id().clone(), first.version().clone(), "d".repeat(64).parse().unwrap()).unwrap()
        );
        let wire = serde_json::to_value(&first).unwrap();
        assert_eq!(wire["version"], "1.0.0+build.a");
        assert_eq!(serde_json::from_value::<InstalledPackageRef>(wire).unwrap(), first);
        for invalid in ["v1.0.0", "1.0", "01.0.0", "1.0.0 "] {
            let mut wire = serde_json::to_value(&first).unwrap();
            wire["version"] = json!(invalid);
            assert!(serde_json::from_value::<InstalledPackageRef>(wire).is_err());
        }
        let version = Version::parse(&format!("1.0.0+{}", "a".repeat(129))).unwrap();
        assert_eq!(InstalledPackageRef::new(first.plugin_id().clone(), version, first.archive_digest().clone()), Err(PolicyError::Version));
    }

    #[test]
    fn bounded_grants_have_canonical_order_and_reject_duplicates() {
        let first = PolicyGrantRequests::new(vec![grant(8), grant(7)]).unwrap();
        let second = PolicyGrantRequests::new(vec![grant(7), grant(8)]).unwrap();
        assert_eq!(first, second);
        assert_eq!(serde_json::to_string(&first).unwrap(), serde_json::to_string(&second).unwrap());
        let wire = serde_json::to_value(&first).unwrap();
        let mut reversed = wire.as_array().unwrap().clone();
        reversed.reverse();
        assert_eq!(serde_json::from_value::<PolicyGrantRequests>(Value::Array(reversed)).unwrap(), first);
        assert_eq!(PolicyGrantRequests::new(vec![grant(7), grant(7)]), Err(PolicyError::DuplicateGrant));
        assert!(
            serde_json::from_value::<PolicyGrantRequests>(json!([
                serde_json::to_value(grant(7)).unwrap(),
                serde_json::to_value(grant(7)).unwrap()
            ]))
            .is_err()
        );
        let limit: Vec<_> = (1..=MAX_POLICY_GRANTS).map(|index| grant(u64::try_from(index).unwrap())).collect();
        assert_eq!(PolicyGrantRequests::new(limit.clone()).unwrap().as_slice().len(), MAX_POLICY_GRANTS);
        let mut overflow = limit;
        overflow.push(grant(999));
        assert_eq!(PolicyGrantRequests::new(overflow.clone()), Err(PolicyError::GrantCount));
        let mut overflow_wire = serde_json::to_value(overflow).unwrap();
        assert!(serde_json::from_value::<PolicyGrantRequests>(overflow_wire.clone()).is_err());
        *overflow_wire.as_array_mut().unwrap().last_mut().unwrap() = json!({"unexpected": "x".repeat(8192)});
        // The excess object must never become a decoded grant request, even if
        // its fields would otherwise trigger unknown-field or size diagnostics.
        let error = serde_json::from_value::<PolicyGrantRequests>(overflow_wire).unwrap_err();
        assert!(error.to_string().contains("too many policy grant requests"));
    }

    #[test]
    fn review_requests_do_not_inherit_missing_grants_or_expected_policy() {
        let original = request();
        let wire = serde_json::to_value(&original).unwrap();
        assert_eq!(serde_json::from_value::<PackageReviewRequest>(wire.clone()).unwrap(), original);
        for field in ["grants", "expected_host_policy", "expected_review", "package", "schema_version", "intent"] {
            let mut value = wire.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PackageReviewRequest>(value).is_err(), "{field}");
        }
        for intent in [PackageReviewIntent::Deny {}, PackageReviewIntent::DataOnly {}] {
            assert_eq!(
                PackageReviewRequest::new(
                    reference(),
                    ExpectedPolicyRevision::Absent {},
                    fingerprint(),
                    intent,
                    PolicyGrantRequests::new(vec![grant(1)]).unwrap()
                ),
                Err(PolicyError::GrantsWithoutExecution)
            );
            assert!(
                PackageReviewRequest::new(
                    reference(),
                    ExpectedPolicyRevision::Absent {},
                    fingerprint(),
                    intent,
                    PolicyGrantRequests::new(Vec::new()).unwrap()
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn all_nested_review_fields_and_empty_intent_variants_are_strict() {
        let original = serde_json::to_value(request()).unwrap();
        for path in ["", "/package", "/expected_review", "/intent", "/grants/0", "/grants/0/scope"] {
            let mut value = original.clone();
            value.pointer_mut(path).unwrap().as_object_mut().unwrap().insert("administrator".to_owned(), json!(true));
            assert!(serde_json::from_value::<PackageReviewRequest>(value).is_err(), "{path}");
        }
        for kind in ["deny", "data_only", "native_full_service_user_trust", "wasm_sandbox_required"] {
            let mut value = original.clone();
            value["intent"] = json!({"kind":kind, "execute":true});
            value["grants"] = json!([]);
            assert!(serde_json::from_value::<PackageReviewRequest>(value).is_err(), "{kind}");
        }
        let mut value = original;
        value["grants"][0]["scope"] = json!({"kind":"all_repositories"});
        assert!(serde_json::from_value::<PackageReviewRequest>(value).is_err());
    }

    #[test]
    fn declaration_checks_use_exact_identity_kind_and_requested_capability_only() {
        let original = request();
        let mut manifest = manifest();
        original.validate_manifest_declarations(&manifest).unwrap();
        manifest.plugin.version = Version::parse("1.0.0+other").unwrap();
        assert_eq!(original.validate_manifest_declarations(&manifest), Err(PolicyError::ManifestIdentity));
        manifest.plugin.version = original.package.version().clone();
        manifest.kind = PackageKind::Wasm { entrypoint: "greeter".parse().unwrap() };
        assert_eq!(original.validate_manifest_declarations(&manifest), Err(PolicyError::PackageKind));
        let mut wasm = original.clone();
        wasm.intent = PackageReviewIntent::WasmSandboxRequired {};
        wasm.validate_manifest_declarations(&manifest).unwrap();
        manifest.plugin.capabilities = Vec::new();
        assert_eq!(wasm.validate_manifest_declarations(&manifest), Err(PolicyError::UndeclaredCapability));
        let mut deny = original.clone();
        deny.intent = PackageReviewIntent::Deny {};
        deny.grants = PolicyGrantRequests::new(Vec::new()).unwrap();
        deny.validate_manifest_declarations(&manifest).unwrap();
        manifest.kind = PackageKind::Data {};
        deny.intent = PackageReviewIntent::DataOnly {};
        deny.validate_manifest_declarations(&manifest).unwrap();
    }

    #[test]
    fn declaration_agreement_does_not_authenticate_archive_linkage() {
        let original = request();
        let mut other = original.clone();
        other.package = InstalledPackageRef::new(
            original.package.plugin_id().clone(),
            original.package.version().clone(),
            "f".repeat(64).parse().unwrap(),
        )
        .unwrap();
        assert_ne!(other.package, original.package);
        // This checks only supplied declarations. Neither Ok result asserts that
        // the supplied manifest belongs to either archive; only the host ledger
        // and actual archive validation can establish that linkage.
        original.validate_manifest_declarations(&manifest()).unwrap();
        other.validate_manifest_declarations(&manifest()).unwrap();
    }

    #[test]
    fn declared_unknown_capability_remains_a_passive_request() {
        let mut manifest = manifest();
        let capability: CapabilityId = "example.unimplemented".parse().unwrap();
        manifest.plugin.capabilities = vec![capability.clone()];
        let mut value = request();
        value.grants = PolicyGrantRequests::new(vec![PolicyGrantRequest { capability, scope: PolicyGrantScope::Instance {} }]).unwrap();
        // Local shape/declaration agreement deliberately does not implement a host
        // catalog or issue a broker handle. The host must still deny unknown calls.
        value.validate_manifest_declarations(&manifest).unwrap();
    }

    #[test]
    fn mutable_requests_require_revalidation_and_fingerprints_are_exact() {
        let mut value = request();
        value.schema_version = POLICY_SCHEMA_VERSION + 1;
        assert_eq!(value.validate(), Err(PolicyError::Schema));
        assert!(serde_json::from_value::<PackageReviewRequest>(serde_json::to_value(value).unwrap()).is_err());
        let mut value = request();
        value.intent = PackageReviewIntent::Deny {};
        assert_eq!(value.validate(), Err(PolicyError::GrantsWithoutExecution));
        assert_ne!(fingerprint(), HostPolicyFingerprint::new("c".repeat(64).parse().unwrap()));
        for invalid in [json!("b".repeat(63)), json!("B".repeat(64)), json!({"digest":"b".repeat(64)})] {
            assert!(serde_json::from_value::<HostPolicyFingerprint>(invalid).is_err());
        }
    }

    #[test]
    fn candidate_selection_requires_explicit_revisions_and_no_extra_authority_fields() {
        let request = PackageSelectionRequest::new(
            reference(),
            ExpectedPolicyRevision::Exact { revision: PolicyRevision::new(4).unwrap() },
            PolicyRevision::new(7).unwrap(),
            fingerprint(),
        );
        let wire = serde_json::to_value(&request).unwrap();
        assert_eq!(serde_json::from_value::<PackageSelectionRequest>(wire.clone()).unwrap(), request);
        for field in ["expected_review_revision", "expected_selection", "expected_host_policy"] {
            let mut value = wire.clone();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PackageSelectionRequest>(value).is_err());
        }
        for path in ["", "/package", "/expected_selection"] {
            let mut value = wire.clone();
            value.pointer_mut(path).unwrap().as_object_mut().unwrap().insert("enabled".to_owned(), json!(true));
            assert!(serde_json::from_value::<PackageSelectionRequest>(value).is_err());
        }
        let mut changed = request;
        changed.schema_version = 0;
        assert_eq!(changed.validate(), Err(PolicyError::Schema));
    }
}
