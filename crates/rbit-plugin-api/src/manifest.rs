//! Bounded plugin declarations and host API compatibility checks.

use std::collections::BTreeSet;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{CapabilityId, PluginId, ServiceId};

/// Required service protocol and accepted versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDependency {
    pub id: ServiceId,
    pub version: VersionReq,
}

/// Service protocol exported by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceProvided {
    pub id: ServiceId,
    pub version: Version,
}

/// Declarative plugin metadata. Capability requests do not grant authority.
///
/// Limits protect graph construction and registry memory. Package readers must separately
/// cap input bytes before deserializing, and validate package paths and integrity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ManifestWire")]
pub struct PluginManifest {
    pub id: PluginId,
    pub version: Version,
    pub host_api: VersionReq,
    pub requires: Vec<ServiceDependency>,
    pub provides: Vec<ServiceProvided>,
    pub capabilities: Vec<CapabilityId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWire {
    id: PluginId,
    version: Version,
    host_api: VersionReq,
    #[serde(default)]
    requires: Vec<ServiceDependency>,
    #[serde(default)]
    provides: Vec<ServiceProvided>,
    #[serde(default)]
    capabilities: Vec<CapabilityId>,
}

impl TryFrom<ManifestWire> for PluginManifest {
    type Error = ManifestError;

    fn try_from(wire: ManifestWire) -> Result<Self, Self::Error> {
        let manifest = Self {
            id: wire.id,
            version: wire.version,
            host_api: wire.host_api,
            requires: wire.requires,
            provides: wire.provides,
            capabilities: wire.capabilities,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

/// Why a declaration cannot be admitted to the host registry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("{field} has more than {max} entries")]
    TooMany { field: &'static str, max: usize },
    #[error("duplicate declaration in {field}: {id}")]
    Duplicate { field: &'static str, id: String },
    #[error("a plugin cannot require a service it also provides: {0}")]
    SelfDependency(ServiceId),
    #[error("{field} exceeds its version expression limit")]
    VersionLimit { field: &'static str },
    #[error("host API version {actual} does not satisfy {required}")]
    HostApi { actual: Version, required: VersionReq },
}

impl PluginManifest {
    /// Maximum declarations in each manifest collection.
    pub const MAX_DECLARATIONS: usize = 128;

    /// Validates metadata assembled in memory as well as deserialized declarations.
    pub fn validate(&self) -> Result<(), ManifestError> {
        check_version(&self.version, "version")?;
        check_requirement(&self.host_api, "host_api")?;
        check_unique("requires", self.requires.iter().map(|service| service.id.as_str()))?;
        check_unique("provides", self.provides.iter().map(|service| service.id.as_str()))?;
        check_unique("capabilities", self.capabilities.iter().map(CapabilityId::as_str))?;
        let provided: BTreeSet<_> = self.provides.iter().map(|service| &service.id).collect();
        for dependency in &self.requires {
            check_requirement(&dependency.version, "requires.version")?;
            if provided.contains(&dependency.id) {
                return Err(ManifestError::SelfDependency(dependency.id.clone()));
            }
        }
        for service in &self.provides {
            check_version(&service.version, "provides.version")?;
        }
        Ok(())
    }

    /// Validates declarations and checks the host API version using semantic version rules.
    ///
    /// A wildcard requirement does not implicitly accept prerelease hosts.
    pub fn check_host(&self, host_api: &Version) -> Result<(), ManifestError> {
        self.validate()?;
        if self.host_api.matches(host_api) {
            Ok(())
        } else {
            Err(ManifestError::HostApi { actual: host_api.clone(), required: self.host_api.clone() })
        }
    }
}

fn check_unique<'a>(field: &'static str, entries: impl Iterator<Item = &'a str>) -> Result<(), ManifestError> {
    let mut seen = BTreeSet::new();
    for (index, entry) in entries.enumerate() {
        if index >= PluginManifest::MAX_DECLARATIONS {
            return Err(ManifestError::TooMany { field, max: PluginManifest::MAX_DECLARATIONS });
        }
        if !seen.insert(entry) {
            return Err(ManifestError::Duplicate { field, id: entry.to_owned() });
        }
    }
    Ok(())
}

fn check_version(version: &Version, field: &'static str) -> Result<(), ManifestError> {
    if version.to_string().len() > 128 {
        return Err(ManifestError::VersionLimit { field });
    }
    Ok(())
}

fn check_requirement(requirement: &VersionReq, field: &'static str) -> Result<(), ManifestError> {
    if requirement.comparators.len() > 16 || requirement.to_string().len() > 512 {
        return Err(ManifestError::VersionLimit { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn wire() -> Value {
        json!({ "id": "rbit.actions", "version": "0.1.5", "host_api": "^1.0", "requires": [{ "id": "rbit.repositories", "version": "^1.0" }], "provides": [{ "id": "rbit.actions", "version": "1.0.0" }], "capabilities": ["repository.read"] })
    }

    #[test]
    fn manifest_round_trip_and_compatible_host() {
        let manifest: PluginManifest = serde_json::from_value(wire()).unwrap();
        manifest.check_host(&Version::new(1, 3, 0)).unwrap();
        assert_eq!(serde_json::from_str::<PluginManifest>(&serde_json::to_string(&manifest).unwrap()).unwrap(), manifest);
        assert!(matches!(manifest.check_host(&Version::new(2, 0, 0)), Err(ManifestError::HostApi { .. })));
        assert!(manifest.check_host(&"1.1.0-beta.1".parse().unwrap()).is_err());
    }

    #[test]
    fn duplicate_declarations_and_self_dependencies_are_rejected() {
        for field in ["requires", "provides", "capabilities"] {
            let mut value = wire();
            let first = value[field][0].clone();
            value[field].as_array_mut().unwrap().push(first);
            assert!(serde_json::from_value::<PluginManifest>(value).is_err(), "{field}");
        }
        let mut value = wire();
        value["requires"][0]["id"] = json!("rbit.actions");
        assert!(serde_json::from_value::<PluginManifest>(value).is_err());
    }

    #[test]
    fn undeclared_fields_are_not_silently_ignored() {
        for nested in [false, true] {
            let mut value = wire();
            if nested {
                value["requires"][0]["optional"] = json!(true);
            } else {
                value["allow_all"] = json!(true);
            }
            assert!(serde_json::from_value::<PluginManifest>(value).is_err());
        }
    }

    #[test]
    fn collections_and_version_metadata_are_bounded() {
        let mut manifest: PluginManifest = serde_json::from_value(wire()).unwrap();
        manifest.capabilities = (0..=PluginManifest::MAX_DECLARATIONS).map(|n| format!("example.cap-{n}").parse().unwrap()).collect();
        assert!(matches!(manifest.validate(), Err(ManifestError::TooMany { field: "capabilities", .. })));

        let mut value = wire();
        value["version"] = json!(format!("1.0.0+{}", "x".repeat(129)));
        assert!(serde_json::from_value::<PluginManifest>(value).is_err());
    }

    #[test]
    fn empty_collections_have_explicit_defaults() {
        let manifest: PluginManifest =
            serde_json::from_value(json!({ "id":"example.minimal", "version":"1.0.0", "host_api":"1" })).unwrap();
        assert!(manifest.requires.is_empty());
        assert!(manifest.provides.is_empty());
        assert!(manifest.capabilities.is_empty());
    }
}
