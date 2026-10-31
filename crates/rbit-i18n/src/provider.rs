use fluent_bundle::{FluentArgs, FluentValue};
use rbit_messages::{ArgumentKind, Direction, LocaleSnapshot, MessageRef, MessageSchema, Messages, Value};
use semver::Version;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    PackError, PackManifest,
    archive::{Archive, hex_digest},
    catalog::{Catalog, MAX_ARGUMENT, MAX_MESSAGES, MAX_OUTPUT},
    locale::{browser_allows, browser_preferences, validate_locale},
};

#[derive(Debug)]
pub struct LanguagePack {
    manifest: PackManifest,
    digest: String,
    catalogs: BTreeMap<String, Catalog>,
    schema_digest: String,
    expanded_bytes: usize,
}
impl LanguagePack {
    pub fn parse(bytes: &[u8], schema: &MessageSchema) -> Result<Self, PackError> {
        let archive = Archive::parse(bytes)?;
        if archive.manifest.locale == "en"
            || !archive.manifest.message_schema.matches(&Version::parse(rbit_messages::SCHEMA_VERSION).map_err(|_| PackError::Schema)?)
        {
            return Err(PackError::Schema);
        }
        let locale = validate_locale(&archive.manifest.locale, Some(&archive.manifest.direction))?;
        if let Some(parent) = &archive.manifest.parent
            && validate_locale(parent, None)?.language != locale.language
        {
            return Err(PackError::Locale);
        }
        let mut catalogs = BTreeMap::new();
        let mut messages = 0usize;
        for (namespace, path) in &archive.manifest.catalogs {
            let text = std::str::from_utf8(&archive.files[path]).map_err(|_| PackError::Catalog)?;
            let catalog = Catalog::parse(text, locale.clone(), namespace, schema)?;
            messages += catalog.keys.len();
            if messages > MAX_MESSAGES {
                return Err(PackError::Limit);
            }
            catalogs.insert(namespace.clone(), catalog);
        }
        Ok(Self {
            manifest: archive.manifest,
            digest: archive.digest,
            catalogs,
            schema_digest: schema_digest(schema),
            expanded_bytes: archive.files.values().map(Vec::len).sum(),
        })
    }
    pub fn manifest(&self) -> &PackManifest {
        &self.manifest
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn message_count(&self) -> usize {
        self.catalogs.values().map(|catalog| catalog.keys.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleInfo {
    pub locale: String,
    pub direction: String,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackSet {
    inner: Arc<Set>,
}
#[derive(Debug)]
struct Set {
    packs: Vec<Arc<LanguagePack>>,
    locales: BTreeMap<String, LocaleInfo>,
    providers: BTreeMap<(String, String), usize>,
    schema: Arc<MessageSchema>,
    english: Arc<dyn Messages>,
    revision: u64,
}
impl PackSet {
    pub fn new(
        packs: Vec<Arc<LanguagePack>>,
        schema: Arc<MessageSchema>,
        english: Arc<dyn Messages>,
        revision: u64,
    ) -> Result<Self, PackError> {
        if packs.len() > 64
            || packs.iter().map(|pack| pack.expanded_bytes).sum::<usize>() > 64 * 1024 * 1024
            || packs.iter().map(|pack| pack.message_count()).sum::<usize>() > 65_536
        {
            return Err(PackError::Limit);
        }
        let expected = schema_digest(&schema);
        let mut locales = BTreeMap::<String, LocaleInfo>::new();
        let mut providers = BTreeMap::new();
        let mut identities = BTreeSet::new();
        for (index, pack) in packs.iter().enumerate() {
            if pack.schema_digest != expected {
                return Err(PackError::Schema);
            }
            if !identities.insert(&pack.manifest.id) {
                return Err(PackError::Conflict);
            }
            let manifest = &pack.manifest;
            let info =
                LocaleInfo { locale: manifest.locale.clone(), direction: manifest.direction.clone(), parent: manifest.parent.clone() };
            if locales.get(&info.locale).is_some_and(|old| old != &info) {
                return Err(PackError::Conflict);
            }
            locales.insert(info.locale.clone(), info);
            for namespace in pack.catalogs.keys() {
                if providers.insert((manifest.locale.clone(), namespace.clone()), index).is_some() {
                    return Err(PackError::Conflict);
                }
            }
        }
        for info in locales.values() {
            let mut seen = BTreeSet::new();
            let mut current = info;
            while let Some(parent) = &current.parent {
                if !seen.insert(&current.locale) {
                    return Err(PackError::Conflict);
                }
                current = locales.get(parent).ok_or(PackError::Conflict)?;
                if seen.len() > 8 {
                    return Err(PackError::Limit);
                }
            }
        }
        Ok(Self { inner: Arc::new(Set { packs, locales, providers, schema, english, revision }) })
    }
    pub fn locales(&self) -> Vec<LocaleInfo> {
        self.inner.locales.values().cloned().collect()
    }
    pub fn snapshot(&self, locale: &str) -> LocaleSnapshot {
        let Some(info) = self.inner.locales.get(locale) else {
            return LocaleSnapshot::new("en".into(), Direction::Ltr, self.inner.revision, Arc::clone(&self.inner.english));
        };
        let mut chain = Vec::new();
        let mut current = info;
        loop {
            chain.push(current.locale.clone());
            let Some(parent) = &current.parent else { break };
            current = &self.inner.locales[parent];
        }
        let direction = if info.direction == "rtl" { Direction::Rtl } else { Direction::Ltr };
        LocaleSnapshot::new(
            info.locale.clone(),
            direction,
            self.inner.revision,
            Arc::new(LocaleProvider { set: Arc::clone(&self.inner), chain }),
        )
    }
    pub fn negotiate(&self, account: Option<&str>, cookie: Option<&str>, accept: Option<&str>, default: &str) -> LocaleSnapshot {
        for preference in [account, cookie].into_iter().flatten() {
            if let Some(locale) = self.match_preference(preference) {
                return self.snapshot(&locale);
            }
        }
        let preferences = accept.map(browser_preferences).unwrap_or_default();
        for preference in &preferences {
            if preference.quality == 0 {
                continue;
            }
            if preference.range == "*" {
                if let Some(locale) = self.match_preference(default).filter(|locale| browser_allows(locale, &preferences)) {
                    return self.snapshot(&locale);
                }
                if let Some(locale) = self.inner.locales.keys().find(|locale| browser_allows(locale, &preferences)) {
                    return self.snapshot(locale);
                }
                if browser_allows("en", &preferences) {
                    return self.snapshot("en");
                }
            } else if let Some(locale) = self.match_preference(&preference.range).filter(|locale| browser_allows(locale, &preferences)) {
                return self.snapshot(&locale);
            }
        }
        self.match_preference(default)
            .filter(|locale| browser_allows(locale, &preferences))
            .map_or_else(|| self.snapshot("en"), |locale| self.snapshot(&locale))
    }

    fn match_preference(&self, preference: &str) -> Option<String> {
        // Preferences may be preserved by the host when unsupported; they never
        // become a filesystem path or an unbounded formatting/negotiation input.
        if preference.len() > 64 {
            return None;
        }
        let parsed: unic_langid::LanguageIdentifier = preference.parse().ok()?;
        // BCP 47 preferences are case-insensitive; only package metadata itself
        // must use canonical spelling for deterministic identities.
        let canonical = parsed.to_string();
        let locale = validate_locale(&canonical, None).ok()?;
        let mut candidate = locale.to_string();
        loop {
            if candidate == "en" || self.inner.locales.contains_key(&candidate) {
                return Some(candidate);
            }
            let (parent, _) = candidate.rsplit_once('-')?;
            candidate.truncate(parent.len());
        }
    }
}

#[derive(Debug)]
struct LocaleProvider {
    set: Arc<Set>,
    chain: Vec<String>,
}
impl Messages for LocaleProvider {
    fn format(&self, message: &MessageRef<'_>) -> String {
        let Some(spec) = self.set.schema.message(message.namespace, message.key) else {
            return bounded(self.set.english.format(message));
        };
        if message.args.len() != spec.args.len() || message.args.len() > 32 {
            return bounded(self.set.english.format(message));
        }
        let mut args = FluentArgs::new();
        let mut seen = BTreeSet::new();
        for argument in message.args {
            if !seen.insert(argument.name) {
                return bounded(self.set.english.format(message));
            }
            match (&argument.value, spec.args.get(argument.name)) {
                (Value::Text(value), Some(ArgumentKind::Text)) if value.len() <= MAX_ARGUMENT => args.set(argument.name, *value),
                (Value::Number(value), Some(ArgumentKind::Number)) if value.is_finite() => {
                    args.set(argument.name, FluentValue::from(*value));
                }
                _ => return bounded(self.set.english.format(message)),
            }
        }
        for locale in &self.chain {
            let Some(index) = self.set.providers.get(&(locale.clone(), message.namespace.to_owned())) else { continue };
            let catalog = &self.set.packs[*index].catalogs[message.namespace];
            let Some(pattern) = catalog.bundle.get_message(message.key).and_then(|message| message.value()) else { continue };
            let mut errors = Vec::new();
            let output = catalog.bundle.format_pattern(pattern, Some(&args), &mut errors);
            if errors.is_empty() && output.len() <= MAX_OUTPUT {
                return output.into_owned();
            }
        }
        bounded(self.set.english.format(message))
    }
}
fn bounded(text: String) -> String {
    if text.len() <= MAX_OUTPUT { text } else { "Message unavailable".into() }
}
fn schema_digest(schema: &MessageSchema) -> String {
    let mut entries: Vec<_> =
        schema.messages().map(|message| format!("{}\0{}\0{:?}", message.namespace, message.key, message.args)).collect();
    entries.sort();
    hex_digest(entries.join("\n").as_bytes())
}
