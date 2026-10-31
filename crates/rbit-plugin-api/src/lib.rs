//! Shared, validated plugin contracts without runtime, HTTP or storage dependencies.
//!
//! Deserialization validates identifiers and manifests. Hosts must also validate manifests
//! constructed or modified by trusted code before admitting them to a registry.

mod identifiers;
mod lifecycle;
mod manifest;
mod ui;

pub use identifiers::{CapabilityId, IdentifierError, PluginId, ServiceId};
pub use lifecycle::{DesiredState, Generation, GenerationError, PluginState};
pub use manifest::{ManifestError, PluginManifest, ServiceDependency, ServiceProvided};

pub use ui::{
    MAX_UI_BYTES, MAX_UI_CONTRIBUTIONS, MAX_UI_LIST_ITEMS, UiContent, UiContribution, UiContributionId, UiContributionSet, UiIcon, UiLink,
    UiMessage, UiNoticeTone, UiSlot, UiTarget, UiTone, UiValidationError, UiVisibility,
};
