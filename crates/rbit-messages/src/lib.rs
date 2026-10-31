//! Small host message contract. English remains available without a catalog engine.

use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{Arc, OnceLock},
};

use serde::{Deserialize, Serialize};

static BUILTIN_SCHEMA: OnceLock<Arc<MessageSchema>> = OnceLock::new();

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const MAX_ARGUMENT_BYTES: usize = 2048;
pub const MAX_OUTPUT_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentKind {
    Text,
    Number,
}

#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    Text(&'a str),
    Number(f64),
}

impl Value<'_> {
    pub const fn kind(self) -> ArgumentKind {
        match self {
            Self::Text(_) => ArgumentKind::Text,
            Self::Number(_) => ArgumentKind::Number,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Argument<'a> {
    pub name: &'a str,
    pub value: Value<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct MessageRef<'a> {
    pub namespace: &'a str,
    pub key: &'a str,
    pub args: &'a [Argument<'a>],
}

impl<'a> MessageRef<'a> {
    pub const fn new(namespace: &'a str, key: &'a str) -> Self {
        Self { namespace, key, args: &[] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageSpec {
    pub namespace: String,
    pub key: String,
    pub english: String,
    #[serde(default)]
    pub args: BTreeMap<String, ArgumentKind>,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid or duplicate message definition: {0}")]
pub struct SchemaError(String);

#[derive(Debug)]
pub struct MessageSchema {
    messages: BTreeMap<String, BTreeMap<String, MessageSpec>>,
}

impl MessageSchema {
    pub fn new(messages: Vec<MessageSpec>) -> Result<Self, SchemaError> {
        let mut result = Self { messages: BTreeMap::new() };
        for message in messages {
            if !identifier(&message.namespace)
                || !identifier(&message.key)
                || message.english.len() > MAX_OUTPUT_BYTES
                || message.args.len() > 16
                || message.args.keys().any(|name| !identifier(name))
            {
                return Err(SchemaError(format!("{}.{}", message.namespace, message.key)));
            }
            let namespace = result.messages.entry(message.namespace.clone()).or_default();
            let key = message.key.clone();
            if namespace.insert(key.clone(), message).is_some() {
                return Err(SchemaError(key));
            }
        }
        Ok(result)
    }

    pub fn builtin() -> Arc<Self> {
        Arc::clone(BUILTIN_SCHEMA.get_or_init(|| {
            let messages: Vec<MessageSpec> = serde_json::from_str(include_str!("catalog.json")).expect("embedded message JSON");
            Arc::new(Self::new(messages).expect("embedded message schema"))
        }))
    }

    pub fn message(&self, namespace: &str, key: &str) -> Option<&MessageSpec> {
        self.messages.get(namespace)?.get(key)
    }
    pub fn messages(&self) -> impl Iterator<Item = &MessageSpec> {
        self.messages.values().flat_map(BTreeMap::values)
    }
    pub fn len(&self) -> usize {
        self.messages.values().map(BTreeMap::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Borrow a fixed English label without allocating on the provider-disabled path.
pub fn english_static(namespace: &str, key: &str) -> Option<&'static str> {
    let _ = MessageSchema::builtin();
    let spec = BUILTIN_SCHEMA.get()?.message(namespace, key)?;
    spec.args.is_empty().then_some(spec.english.as_str())
}

/// Implementations return ordinary text; the host owns HTML escaping and markup.
pub trait Messages: Debug + Send + Sync {
    fn format(&self, message: &MessageRef<'_>) -> String;
}

#[derive(Debug, Clone)]
pub struct English {
    schema: Arc<MessageSchema>,
}

impl Default for English {
    fn default() -> Self {
        Self::new(MessageSchema::builtin())
    }
}

impl English {
    pub const fn new(schema: Arc<MessageSchema>) -> Self {
        Self { schema }
    }
}

impl Messages for English {
    fn format(&self, message: &MessageRef<'_>) -> String {
        let Some(spec) = self.schema.message(message.namespace, message.key) else { return bounded(message.key, MAX_OUTPUT_BYTES) };
        if message.args.len() != spec.args.len()
            || message.args.iter().enumerate().any(|(index, arg)| {
                spec.args.get(arg.name) != Some(&arg.value.kind())
                    || message.args[..index].iter().any(|other| other.name == arg.name)
                    || matches!(arg.value, Value::Number(value) if !value.is_finite())
            })
        {
            return spec.english.clone();
        }
        // Expand only the original template. An argument cannot introduce another placeholder.
        let mut rest = spec.english.as_str();
        let mut output = String::new();
        while let Some(start) = rest.find('{') {
            output.push_str(&rest[..start]);
            let Some(end) = rest[start..].find('}') else {
                output.push_str(&rest[start..]);
                return bounded(&output, MAX_OUTPUT_BYTES);
            };
            let name = rest[start + 1..start + end].trim().trim_start_matches('$').trim();
            if let Some(arg) = message.args.iter().find(|arg| arg.name == name) {
                match arg.value {
                    Value::Text(text) => output.push_str(&bounded(text, MAX_ARGUMENT_BYTES)),
                    Value::Number(number) => output.push_str(&number.to_string()),
                }
            } else {
                output.push_str(&rest[start..=start + end]);
            }
            if output.len() >= MAX_OUTPUT_BYTES {
                return bounded(&output, MAX_OUTPUT_BYTES);
            }
            rest = &rest[start + end + 1..];
        }
        output.push_str(rest);
        bounded(&output, MAX_OUTPUT_BYTES)
    }
}

pub fn bounded(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

/// One request's immutable locale and catalog identity, including after replacement.
#[derive(Debug, Clone)]
pub struct LocaleSnapshot {
    lang: String,
    direction: Direction,
    revision: u64,
    provider: Arc<dyn Messages>,
}

impl Default for LocaleSnapshot {
    fn default() -> Self {
        Self::english(0)
    }
}

impl LocaleSnapshot {
    pub fn new(lang: String, direction: Direction, revision: u64, provider: Arc<dyn Messages>) -> Self {
        Self { lang, direction, revision, provider }
    }
    pub fn english(revision: u64) -> Self {
        Self::new("en".into(), Direction::Ltr, revision, Arc::new(English::default()))
    }
    pub fn lang(&self) -> &str {
        &self.lang
    }
    pub const fn direction(&self) -> &'static str {
        self.direction.as_str()
    }
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    pub fn format(&self, message: &MessageRef<'_>) -> String {
        self.provider.format(message)
    }
    pub fn text(&self, namespace: &str, key: &str) -> String {
        self.format(&MessageRef::new(namespace, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_available_without_a_catalog_provider() {
        let schema = MessageSchema::builtin();
        assert!(!schema.is_empty());
        assert_eq!(LocaleSnapshot::english(2).text("core", "nav-explore"), "Explore");
        assert_eq!(LocaleSnapshot::english(2).lang(), "en");
    }

    #[test]
    fn arguments_are_typed_bounded_and_never_reinterpreted() {
        let schema = Arc::new(
            MessageSchema::new(vec![MessageSpec {
                namespace: "test".into(),
                key: "greeting".into(),
                english: "Hello {name}: {count}".into(),
                args: [("name".into(), ArgumentKind::Text), ("count".into(), ArgumentKind::Number)].into(),
            }])
            .unwrap(),
        );
        let english = English::new(schema);
        let args =
            [Argument { name: "name", value: Value::Text("{count}<script>") }, Argument { name: "count", value: Value::Number(3.0) }];
        assert_eq!(english.format(&MessageRef { namespace: "test", key: "greeting", args: &args }), "Hello {count}<script>: 3");
        assert_eq!(bounded("€€", 4), "€");
    }

    #[test]
    fn duplicate_ids_and_invalid_names_are_rejected() {
        let spec = MessageSpec { namespace: "core".into(), key: "save".into(), english: "Save".into(), args: BTreeMap::new() };
        assert!(MessageSchema::new(vec![spec.clone(), spec]).is_err());
    }
}
