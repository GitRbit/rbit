//! Declarative package contracts for a future host-owned installer.
//!
//! These declarations describe regular payload files and requested resources. They
//! neither grant capabilities nor establish publisher trust, verify file contents,
//! certify executable formats, or authorize execution. An installer must separately
//! reject unsafe archive entries, check actual sizes and hashes, and enforce policy.
//!
//! Readers must cap the complete manifest input at [`MAX_PACKAGE_MANIFEST_BYTES`]
//! before deserializing, including the embedded [`PluginManifest`]. File collections
//! are additionally bounded while deserializing. This module does not parse archives.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, IgnoredAny, SeqAccess, Visitor},
};

use crate::{ManifestError, PluginManifest};

pub const PACKAGE_SCHEMA_VERSION: u32 = 1;
pub const MAX_PACKAGE_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_PACKAGE_FILES: usize = 1024;
pub const MAX_PACKAGE_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 1024 * 1024 * 1024;

/// Invalid package metadata; no error contains payload contents or credentials.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    #[error("unsupported package schema version: {0}")]
    SchemaVersion(u32),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("invalid portable package path")]
    Path,
    #[error("invalid native package target")]
    NativeTarget,
    #[error("SHA256 digest must contain exactly 64 lowercase hexadecimal characters")]
    Digest,
    #[error("package file count must be between 1 and {MAX_PACKAGE_FILES}")]
    FileCount,
    #[error("package file exceeds its size limit")]
    FileSize,
    #[error("package exceeds its aggregate size limit")]
    TotalSize,
    #[error("package file paths conflict")]
    FileConflict,
    #[error("package entrypoint must identify an exact declared nonempty file")]
    Entrypoint,
    #[error("requested resource budget exceeds the package contract: {0}")]
    Budget(&'static str),
}

/// A portable relative file path, with one unambiguous spelling.
///
/// Paths are ASCII and slash-separated; empty/dot/parent components, platform
/// separators, Windows device names and trailing dots/spaces are rejected.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PackagePath(String);

impl PackagePath {
    pub const MAX_BYTES: usize = 256;
    pub const MAX_COMPONENT_BYTES: usize = 128;
    pub const MAX_COMPONENTS: usize = 16;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for PackagePath {
    type Err = PackageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || value.len() > Self::MAX_BYTES || !value.is_ascii() {
            return Err(PackageError::Path);
        }
        let mut count = 0;
        for component in value.split('/') {
            count += 1;
            if count > Self::MAX_COMPONENTS
                || component.is_empty()
                || component.len() > Self::MAX_COMPONENT_BYTES
                || matches!(component, "." | "..")
                || component.ends_with(['.', ' '])
                || !component.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(PackageError::Path);
            }
            let stem = component.split('.').next().unwrap_or_default().to_ascii_lowercase();
            if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
                || (stem.len() == 4 && (stem.starts_with("com") || stem.starts_with("lpt")) && matches!(stem.as_bytes()[3], b'1'..=b'9'))
            {
                return Err(PackageError::Path);
            }
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for PackagePath {
    type Error = PackageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for PackagePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_validated_string(deserializer, "a portable relative package file path")
    }
}

/// A declarative native target triple or quadruple, not a host support decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct NativeTarget(String);
impl NativeTarget {
    pub const MAX_BYTES: usize = 128;
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for NativeTarget {
    type Err = PackageError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || value.len() > Self::MAX_BYTES {
            return Err(PackageError::NativeTarget);
        }
        let segments: Vec<_> = value.split('-').collect();
        if !(3..=4).contains(&segments.len())
            || matches!(segments[0], "wasm32" | "wasm64")
            || segments.iter().any(|segment| {
                segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            })
        {
            return Err(PackageError::NativeTarget);
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for NativeTarget {
    type Error = PackageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl fmt::Display for NativeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for NativeTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_validated_string(deserializer, "a bounded native target triple or quadruple")
    }
}

/// Canonical SHA256 syntax only. Actual payload verification belongs to the installer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);
impl Sha256Digest {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for Sha256Digest {
    type Err = PackageError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')) {
            return Err(PackageError::Digest);
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for Sha256Digest {
    type Error = PackageError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_validated_string(deserializer, "64 lowercase hexadecimal SHA256 characters")
    }
}

fn deserialize_validated_string<'de, D, T>(deserializer: D, expected: &'static str) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    struct ValidatedString<T> {
        expected: &'static str,
        marker: std::marker::PhantomData<T>,
    }
    impl<T: FromStr> Visitor<'_> for ValidatedString<T>
    where
        T::Err: fmt::Display,
    {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.expected)
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
            value.parse().map_err(E::custom)
        }
    }
    deserializer.deserialize_str(ValidatedString { expected, marker: std::marker::PhantomData })
}

/// Execution adapter declaration. Data packages have no executable entrypoint.
///
/// Declaring native code does not grant trust. Declaring Wasm does not establish
/// a sandbox: the host must choose and enforce an appropriate execution adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageKind {
    Native { target: NativeTarget, entrypoint: PackagePath },
    Wasm { entrypoint: PackagePath },
    Data {},
}
impl PackageKind {
    pub fn entrypoint(&self) -> Option<&PackagePath> {
        match self {
            Self::Native { entrypoint, .. } | Self::Wasm { entrypoint } => Some(entrypoint),
            Self::Data {} => None,
        }
    }
}

/// One declared regular payload file. Archive metadata is not trusted by this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackageFileWire")]
pub struct PackageFile {
    pub path: PackagePath,
    pub size: u64,
    pub sha256: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFileWire {
    path: PackagePath,
    size: u64,
    sha256: Sha256Digest,
}
impl TryFrom<PackageFileWire> for PackageFile {
    type Error = PackageError;
    fn try_from(wire: PackageFileWire) -> Result<Self, Self::Error> {
        let file = Self { path: wire.path, size: wire.size, sha256: wire.sha256 };
        file.validate()?;
        Ok(file)
    }
}
impl PackageFile {
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.size > MAX_PACKAGE_FILE_BYTES {
            return Err(PackageError::FileSize);
        }
        Ok(())
    }
}

/// Requested runtime/storage ceilings, always intersected with host policy later.
///
/// These are metadata, not grants or enforcement guarantees. In particular, a
/// trusted native process is not sandboxed by declaring a memory budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackageBudgetsWire")]
pub struct PackageBudgets {
    pub memory_bytes: u64,
    pub concurrent_calls: u32,
    pub call_timeout_ms: u32,
    pub storage_bytes: u64,
    pub event_queue_capacity: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageBudgetsWire {
    memory_bytes: u64,
    concurrent_calls: u32,
    call_timeout_ms: u32,
    storage_bytes: u64,
    event_queue_capacity: u32,
}
impl TryFrom<PackageBudgetsWire> for PackageBudgets {
    type Error = PackageError;
    fn try_from(wire: PackageBudgetsWire) -> Result<Self, Self::Error> {
        let budgets = Self {
            memory_bytes: wire.memory_bytes,
            concurrent_calls: wire.concurrent_calls,
            call_timeout_ms: wire.call_timeout_ms,
            storage_bytes: wire.storage_bytes,
            event_queue_capacity: wire.event_queue_capacity,
        };
        budgets.validate()?;
        Ok(budgets)
    }
}
impl PackageBudgets {
    pub const MAX_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
    pub const MAX_CONCURRENT_CALLS: u32 = 128;
    pub const MAX_CALL_TIMEOUT_MS: u32 = 30_000;
    pub const MAX_STORAGE_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
    pub const MAX_EVENT_QUEUE_CAPACITY: u32 = 4096;

    pub fn validate(&self) -> Result<(), PackageError> {
        if self.memory_bytes == 0 || self.memory_bytes > Self::MAX_MEMORY_BYTES {
            return Err(PackageError::Budget("memory_bytes"));
        }
        if self.concurrent_calls == 0 || self.concurrent_calls > Self::MAX_CONCURRENT_CALLS {
            return Err(PackageError::Budget("concurrent_calls"));
        }
        if self.call_timeout_ms == 0 || self.call_timeout_ms > Self::MAX_CALL_TIMEOUT_MS {
            return Err(PackageError::Budget("call_timeout_ms"));
        }
        if self.storage_bytes > Self::MAX_STORAGE_BYTES {
            return Err(PackageError::Budget("storage_bytes"));
        }
        if self.event_queue_capacity > Self::MAX_EVENT_QUEUE_CAPACITY {
            return Err(PackageError::Budget("event_queue_capacity"));
        }
        Ok(())
    }
}

/// Versioned package metadata, validated both on deserialize and before host use.
///
/// Public fields allow trusted composition code to assemble declarations. Always
/// call [`Self::validate`] again after programmatic construction or modification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PackageManifestWire")]
pub struct PackageManifest {
    pub schema_version: u32,
    pub plugin: PluginManifest,
    pub kind: PackageKind,
    pub files: Vec<PackageFile>,
    pub budgets: PackageBudgets,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageManifestWire {
    schema_version: u32,
    plugin: PluginManifest,
    kind: PackageKind,
    #[serde(deserialize_with = "deserialize_files")]
    files: Vec<PackageFile>,
    budgets: PackageBudgets,
}
impl TryFrom<PackageManifestWire> for PackageManifest {
    type Error = PackageError;
    fn try_from(wire: PackageManifestWire) -> Result<Self, Self::Error> {
        let manifest =
            Self { schema_version: wire.schema_version, plugin: wire.plugin, kind: wire.kind, files: wire.files, budgets: wire.budgets };
        manifest.validate()?;
        Ok(manifest)
    }
}
impl PackageManifest {
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.schema_version != PACKAGE_SCHEMA_VERSION {
            return Err(PackageError::SchemaVersion(self.schema_version));
        }
        self.plugin.validate()?;
        self.budgets.validate()?;
        if self.files.is_empty() || self.files.len() > MAX_PACKAGE_FILES {
            return Err(PackageError::FileCount);
        }
        let mut paths = BTreeSet::new();
        let mut directories = BTreeMap::new();
        let mut total = 0_u64;
        for file in &self.files {
            file.validate()?;
            total = total.checked_add(file.size).filter(|size| *size <= MAX_PACKAGE_BYTES).ok_or(PackageError::TotalSize)?;
            let original = file.path.as_str();
            for (index, _) in original.match_indices('/') {
                let directory = &original[..index];
                if let Some(previous) = directories.insert(directory.to_ascii_lowercase(), directory)
                    && previous != directory
                {
                    return Err(PackageError::FileConflict);
                }
            }
            let path = original.to_ascii_lowercase();
            if !paths.insert(path) {
                return Err(PackageError::FileConflict);
            }
        }
        for path in &paths {
            for (index, _) in path.match_indices('/') {
                if paths.contains(&path[..index]) {
                    return Err(PackageError::FileConflict);
                }
            }
        }
        if let Some(entrypoint) = self.kind.entrypoint()
            && !self.files.iter().any(|file| &file.path == entrypoint && file.size > 0)
        {
            return Err(PackageError::Entrypoint);
        }
        Ok(())
    }
}

fn deserialize_files<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PackageFile>, D::Error> {
    struct Files;
    impl<'de> Visitor<'de> for Files {
        type Value = Vec<PackageFile>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "between 1 and {MAX_PACKAGE_FILES} regular payload declarations")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|count| count > MAX_PACKAGE_FILES) {
                return Err(de::Error::custom(PackageError::FileCount));
            }
            let mut files = Vec::new();
            while files.len() < MAX_PACKAGE_FILES {
                let Some(file) = sequence.next_element()? else { return Ok(files) };
                files.push(file);
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(PackageError::FileCount));
            }
            Ok(files)
        }
    }
    deserializer.deserialize_seq(Files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn wire() -> Value {
        json!({
            "schema_version": 1,
            "plugin": { "id": "example.package", "version": "1.0.0", "host_api": "^1" },
            "kind": { "type": "native", "target": "x86_64-unknown-linux-gnu", "entrypoint": "bin/plugin" },
            "files": [ { "path": "bin/plugin", "size": 1, "sha256": "a".repeat(64) } ],
            "budgets": { "memory_bytes": 1024 * 1024, "concurrent_calls": 1, "call_timeout_ms": 1000, "storage_bytes": 0, "event_queue_capacity": 0 }
        })
    }
    fn manifest() -> PackageManifest {
        serde_json::from_value(wire()).unwrap()
    }

    #[test]
    fn all_package_kinds_round_trip_without_granting_execution() {
        for kind in [
            json!({ "type":"native", "target":"aarch64-apple-darwin", "entrypoint":"bin/plugin" }),
            json!({ "type":"wasm", "entrypoint":"bin/plugin" }),
            json!({ "type":"data" }),
        ] {
            let mut value = wire();
            value["kind"] = kind;
            let manifest: PackageManifest = serde_json::from_value(value).unwrap();
            manifest.validate().unwrap();
            assert_eq!(serde_json::from_str::<PackageManifest>(&serde_json::to_string(&manifest).unwrap()).unwrap(), manifest);
        }
    }

    #[test]
    fn paths_reject_hostile_and_platform_ambiguous_spellings() {
        for path in [
            "",
            "/absolute",
            "//server/share",
            "a//b",
            "a/",
            ".",
            "..",
            "a/./b",
            "a/../b",
            "../b",
            "a\\b",
            "C:drive",
            "a:stream",
            "a b",
            "a\0b",
            "a\nb",
            "a\tb",
            "a\u{7f}b",
            "café",
            "a.",
            "a /b",
            "a./b",
            "CON",
            "con.txt",
            "AUX.anything",
            "prn",
            "nul",
            "com1",
            "COM9.bin",
            "LPT1.txt",
            "lpt9",
            "CONIN$",
            "CONOUT$",
            "a?b",
            "a*b",
            "a%2fb",
        ] {
            assert_eq!(path.parse::<PackagePath>(), Err(PackageError::Path), "{path:?}");
            assert!(serde_json::from_value::<PackagePath>(json!(path)).is_err(), "{path:?}");
        }
        for path in ["bin/plugin.exe", "assets/catalog.en.json", "a-b/c_d", ".metadata", "com0", "lpt10", "console.txt"] {
            assert_eq!(path.parse::<PackagePath>().unwrap().as_str(), path);
        }
        assert!("a".repeat(PackagePath::MAX_COMPONENT_BYTES + 1).parse::<PackagePath>().is_err());
        assert!(vec!["a"; PackagePath::MAX_COMPONENTS + 1].join("/").parse::<PackagePath>().is_err());
        assert!(format!("{}/{}", "a".repeat(128), "b".repeat(128)).parse::<PackagePath>().is_err());
    }

    #[test]
    fn native_targets_and_digests_are_canonical_and_bounded() {
        for target in [
            "",
            "x86_64",
            "x86_64-linux",
            "../x86_64-linux-gnu",
            "x86_64/unknown-linux-gnu",
            "x86_64--linux-gnu",
            "X86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu extra",
            "wasm32-unknown-unknown",
            "wasm64-unknown-unknown",
        ] {
            assert!(target.parse::<NativeTarget>().is_err(), "{target}");
        }
        assert!("a".repeat(NativeTarget::MAX_BYTES + 1).parse::<NativeTarget>().is_err());
        for target in ["x86_64-pc-windows-msvc", "aarch64-apple-darwin", "aarch64-unknown-linux-musl"] {
            assert_eq!(target.parse::<NativeTarget>().unwrap().as_str(), target);
        }
        for digest in [String::new(), "a".repeat(63), "a".repeat(65), "A".repeat(64), "g".repeat(64), format!("{}\n", "a".repeat(63))] {
            assert_eq!(digest.parse::<Sha256Digest>(), Err(PackageError::Digest));
            assert!(serde_json::from_value::<Sha256Digest>(json!(digest)).is_err());
        }
        assert!("0123456789abcdef".repeat(4).parse::<Sha256Digest>().is_ok());
    }

    #[test]
    fn conflicting_file_names_and_ancestors_fail_in_either_order() {
        for paths in [
            ["bin/plugin", "bin/plugin"],
            ["bin/plugin", "BIN/PLUGIN"],
            ["bin", "bin/plugin"],
            ["BIN", "bin/plugin"],
            ["bin/plugin", "bin"],
            ["bin/plugin", "BIN/other"],
            ["BIN/other", "bin/plugin"],
            ["Bin/Sub/x", "Bin/sub/y"],
        ] {
            let mut manifest = manifest();
            manifest.kind = PackageKind::Data {};
            manifest.files = paths
                .into_iter()
                .map(|path| PackageFile { path: path.parse().unwrap(), size: 1, sha256: "a".repeat(64).parse().unwrap() })
                .collect();
            assert_eq!(manifest.validate(), Err(PackageError::FileConflict));
            assert!(serde_json::from_value::<PackageManifest>(serde_json::to_value(&manifest).unwrap()).is_err());
        }
    }

    #[test]
    fn consistently_spelled_shared_directories_are_allowed() {
        let mut manifest = manifest();
        manifest.files.push(PackageFile { path: "bin/other".parse().unwrap(), size: 0, sha256: "b".repeat(64).parse().unwrap() });
        manifest.validate().unwrap();
    }

    #[test]
    fn entrypoints_require_an_exact_nonempty_declared_file() {
        for path in ["missing", "BIN/plugin", "bin", "bin/plugin/missing"] {
            let mut manifest = manifest();
            manifest.kind = PackageKind::Wasm { entrypoint: path.parse().unwrap() };
            assert_eq!(manifest.validate(), Err(PackageError::Entrypoint));
        }
        let mut manifest = manifest();
        manifest.files[0].size = 0;
        assert_eq!(manifest.validate(), Err(PackageError::Entrypoint));
        manifest.kind = PackageKind::Data {};
        manifest.validate().unwrap();
    }

    #[test]
    fn programmatic_mutations_revalidate_schema_plugin_file_and_aggregate_limits() {
        let mut value = manifest();
        value.schema_version = 2;
        assert_eq!(value.validate(), Err(PackageError::SchemaVersion(2)));
        value = manifest();
        value.plugin.capabilities = vec!["example.read".parse().unwrap(); 2];
        assert!(matches!(value.validate(), Err(PackageError::Manifest(_))));
        value = manifest();
        value.files[0].size = u64::MAX;
        assert_eq!(value.validate(), Err(PackageError::FileSize));
        value = manifest();
        value.files.clear();
        assert_eq!(value.validate(), Err(PackageError::FileCount));
        value = manifest();
        value.files = (0..=MAX_PACKAGE_FILES)
            .map(|index| PackageFile { path: format!("file-{index}").parse().unwrap(), size: 1, sha256: "a".repeat(64).parse().unwrap() })
            .collect();
        assert_eq!(value.validate(), Err(PackageError::FileCount));
        value.kind = PackageKind::Data {};
        value.files.truncate(4);
        for file in &mut value.files {
            file.size = MAX_PACKAGE_FILE_BYTES;
        }
        value.validate().unwrap();
        value.files.push(PackageFile { path: "overflow".parse().unwrap(), size: 1, sha256: "a".repeat(64).parse().unwrap() });
        assert_eq!(value.validate(), Err(PackageError::TotalSize));
    }

    #[test]
    fn serde_rejects_unknown_missing_and_unsupported_declarations() {
        for field in ["schema_version", "plugin", "kind", "files", "budgets"] {
            let mut value = wire();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err(), "{field}");
        }
        for selector in ["", "plugin", "kind", "budgets", "file"] {
            let mut value = wire();
            let object = match selector {
                "" => &mut value,
                "file" => &mut value["files"][0],
                field => &mut value[field],
            };
            object["unexpected"] = json!(true);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err(), "{selector}");
        }
        for version in [0, 2, u32::MAX] {
            let mut value = wire();
            value["schema_version"] = json!(version);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err());
        }
        for kind in [
            json!({"type":"native", "target":"x86_64-unknown-linux-gnu"}),
            json!({"type":"native", "entrypoint":"bin/plugin"}),
            json!({"type":"wasm"}),
        ] {
            let mut value = wire();
            value["kind"] = kind;
            assert!(serde_json::from_value::<PackageManifest>(value).is_err());
        }
        let mut value = wire();
        value["kind"] = json!({"type":"data", "entrypoint":"bin/plugin"});
        assert!(serde_json::from_value::<PackageManifest>(value).is_err());
        let mut value = wire();
        value["kind"]["type"] = json!("shared_library");
        assert!(serde_json::from_value::<PackageManifest>(value).is_err());
    }

    #[test]
    fn serde_rejects_excessive_file_arrays_and_sizes() {
        let mut value = wire();
        value["files"] = json!(
            (0..=MAX_PACKAGE_FILES)
                .map(|index| json!({"path":format!("file-{index}"), "size":1, "sha256":"a".repeat(64)}))
                .collect::<Vec<_>>()
        );
        // JSON streaming has no sequence size hint; it must enforce the same count cap.
        assert!(serde_json::from_str::<PackageManifest>(&serde_json::to_string(&value).unwrap()).is_err());
        assert!(serde_json::from_value::<PackageManifest>(value).is_err());
        for size in [MAX_PACKAGE_FILE_BYTES + 1, u64::MAX] {
            let mut value = wire();
            value["files"][0]["size"] = json!(size);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err());
        }
    }

    #[test]
    fn budget_requests_have_no_defaults_or_unbounded_values() {
        for (field, invalid) in [
            ("memory_bytes", 0),
            ("memory_bytes", PackageBudgets::MAX_MEMORY_BYTES + 1),
            ("concurrent_calls", 0),
            ("concurrent_calls", u64::from(PackageBudgets::MAX_CONCURRENT_CALLS) + 1),
            ("call_timeout_ms", 0),
            ("call_timeout_ms", u64::from(PackageBudgets::MAX_CALL_TIMEOUT_MS) + 1),
            ("storage_bytes", PackageBudgets::MAX_STORAGE_BYTES + 1),
            ("event_queue_capacity", u64::from(PackageBudgets::MAX_EVENT_QUEUE_CAPACITY) + 1),
        ] {
            let mut value = wire();
            value["budgets"][field] = json!(invalid);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err(), "{field}={invalid}");
        }
        let mut value = manifest();
        value.budgets.memory_bytes = 0;
        assert_eq!(value.validate(), Err(PackageError::Budget("memory_bytes")));
        for field in ["memory_bytes", "concurrent_calls", "call_timeout_ms", "storage_bytes", "event_queue_capacity"] {
            let mut value = wire();
            value["budgets"].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PackageManifest>(value).is_err(), "{field}");
        }
    }
}
