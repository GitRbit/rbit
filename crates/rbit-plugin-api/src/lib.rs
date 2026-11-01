//! Shared, validated plugin contracts without runtime, HTTP or storage dependencies.
//!
//! Deserialization validates identifiers and manifests. Hosts must also validate manifests
//! constructed or modified by trusted code before admitting them to a registry.

mod identifiers;
mod lifecycle;
mod manifest;
mod package;
mod policy;
mod ui;

pub use identifiers::{CapabilityId, IdentifierError, PluginId, ServiceId};
pub use lifecycle::{DesiredState, Generation, GenerationError, PluginState};
pub use manifest::{ManifestError, PluginManifest, ServiceDependency, ServiceProvided};
pub use package::{
    MAX_PACKAGE_BYTES, MAX_PACKAGE_FILE_BYTES, MAX_PACKAGE_FILES, MAX_PACKAGE_MANIFEST_BYTES, NativeTarget, PACKAGE_SCHEMA_VERSION,
    PackageBudgets, PackageError, PackageFile, PackageKind, PackageManifest, PackagePath, Sha256Digest,
};

pub use ui::{
    MAX_UI_BYTES, MAX_UI_CONTRIBUTIONS, MAX_UI_LIST_ITEMS, UiContent, UiContribution, UiContributionId, UiContributionSet, UiIcon, UiLink,
    UiMessage, UiNoticeTone, UiSlot, UiTarget, UiTone, UiValidationError, UiVisibility,
};

pub use policy::{
    ExpectedPolicyRevision, HostPolicyFingerprint, InstalledPackageRef, MAX_POLICY_GRANTS, MAX_POLICY_RENEWAL_REQUEST_BYTES,
    MAX_POLICY_REQUEST_BYTES, POLICY_SCHEMA_VERSION, PackagePolicyRenewalRequest, PackageReviewIntent, PackageReviewRequest,
    PackageSelectionClearRequest, PackageSelectionRequest, PolicyError, PolicyGrantRequest, PolicyGrantRequests, PolicyGrantScope,
    PolicyRevision, RepositoryGrantId,
};
