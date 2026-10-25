//! A YAML document as a tree of [`Node`]s that remember where they came from.
//!
//! Parsing goes through `serde-saphyr`, which is panic-free and enforces budgets (nesting
//! depth, node and alias counts, alias replays), so a hostile file cannot exhaust memory or
//! the stack. Aliases and merge keys are resolved; the tree holds plain data.
//!
//! A scalar keeps both its value, as YAML's core schema resolves it, and — when it was
//! written plain on one line — its source text. Re-emitting that text verbatim (see
//! [`crate::payload`]) lets a runner read `3.10`, `yes` or `0x1F` exactly as it would have
//! read the original file, whatever its own YAML library makes of them.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_saphyr::Spanned;

use crate::{
    error::{ErrorKind, Location, WorkflowError},
    limits::Limits,
};

/// One node of a document.
#[derive(Debug, Clone)]
pub struct Node {
    pub value: Value,
    /// Where the node is used (for an alias, the alias).
    pub at: Location,
}

#[derive(Debug, Clone)]
pub enum Value {
    Scalar(Scalar),
    Sequence(Vec<Node>),
    Mapping(Mapping),
}

/// A mapping with its keys in document order. Keys are strings: workflows use no others.
#[derive(Debug, Clone, Default)]
pub struct Mapping(pub Vec<(Key, Node)>);

#[derive(Debug, Clone)]
pub struct Key {
    pub name: String,
    pub at: Location,
}

#[derive(Debug, Clone)]
pub struct Scalar {
    pub value: ScalarValue,
    /// The source text when the scalar was written plain on one line, without a tag.
    pub plain: Option<String>,
    /// Byte range of the definition in the source, used to recover `plain`.
    span: Option<(usize, usize)>,
}

/// A scalar's value as YAML's core schema resolves it.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    String(String),
}

impl Scalar {
    pub fn new(value: ScalarValue) -> Self {
        Self { value, plain: None, span: None }
    }

    pub fn string(text: impl Into<String>) -> Self {
        Self::new(ScalarValue::String(text.into()))
    }

    /// What a YAML library decoding this scalar into a string sees: the source text of a
    /// plain scalar (`3.10` stays `3.10`), otherwise the value.
    pub fn text(&self) -> String {
        if let Some(plain) = &self.plain {
            return plain.clone();
        }
        match &self.value {
            ScalarValue::Null => String::new(),
            ScalarValue::Bool(value) => value.to_string(),
            ScalarValue::Int(value) => value.to_string(),
            ScalarValue::Float(value) => format_float(*value),
            ScalarValue::String(value) => value.clone(),
        }
    }
}

/// Floats as Go's `fmt` prints them for the common cases: `1.5`, `3`, `1e+21`.
pub(crate) fn format_float(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { ".inf".to_owned() } else { "-.inf".to_owned() }
    } else {
        format!("{value}")
    }
}

impl Node {
    pub fn new(value: Value, at: Location) -> Self {
        Self { value, at }
    }

    pub fn scalar(scalar: Scalar, at: Location) -> Self {
        Self::new(Value::Scalar(scalar), at)
    }

    pub fn as_scalar(&self) -> Option<&Scalar> {
        if let Value::Scalar(scalar) = &self.value { Some(scalar) } else { None }
    }

    pub fn as_mapping(&self) -> Option<&Mapping> {
        if let Value::Mapping(mapping) = &self.value { Some(mapping) } else { None }
    }

    pub fn as_sequence(&self) -> Option<&[Node]> {
        if let Value::Sequence(items) = &self.value { Some(items) } else { None }
    }

    pub fn is_null(&self) -> bool {
        matches!(&self.value, Value::Scalar(Scalar { value: ScalarValue::Null, .. }))
    }

    /// The scalar as a string, if it is a non-null scalar.
    pub fn as_text(&self) -> Option<String> {
        self.as_scalar().filter(|scalar| scalar.value != ScalarValue::Null).map(Scalar::text)
    }

    /// The node as JSON data, as an expression sees it. Integers stay integers where JSON
    /// can hold them; non-finite floats, which JSON cannot, become `null`.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::Value as Json;
        match &self.value {
            Value::Scalar(scalar) => match &scalar.value {
                ScalarValue::Null => Json::Null,
                ScalarValue::Bool(value) => Json::Bool(*value),
                #[allow(clippy::cast_precision_loss, reason = "beyond 64 bits, only the magnitude of a number is meaningful")]
                ScalarValue::Int(value) => i64::try_from(*value)
                    .map(Json::from)
                    .or_else(|_| u64::try_from(*value).map(Json::from))
                    .unwrap_or_else(|_| serde_json::Number::from_f64(*value as f64).map_or(Json::Null, Json::Number)),
                ScalarValue::Float(value) => serde_json::Number::from_f64(*value).map_or(Json::Null, Json::Number),
                ScalarValue::String(text) => Json::String(text.clone()),
            },
            Value::Sequence(items) => Json::Array(items.iter().map(Self::to_json).collect()),
            Value::Mapping(mapping) => Json::Object(mapping.iter().map(|(key, value)| (key.name.clone(), value.to_json())).collect()),
        }
    }

    /// Equality of data, ignoring locations and spelling (`1.10` equals `1.1`, `1` does not
    /// equal `1.0` or `"1"`), as Go's `reflect.DeepEqual` compares decoded values.
    pub fn same_value(&self, other: &Self) -> bool {
        match (&self.value, &other.value) {
            (Value::Scalar(a), Value::Scalar(b)) => a.value == b.value,
            (Value::Sequence(a), Value::Sequence(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same_value(b)),
            (Value::Mapping(a), Value::Mapping(b)) => {
                a.0.len() == b.0.len() && a.0.iter().all(|(key, value)| b.get(&key.name).is_some_and(|other| value.same_value(other)))
            }
            _ => false,
        }
    }
}

impl Mapping {
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.0.iter().find(|(key, _)| key.name == name).map(|(_, node)| node)
    }

    pub fn entry(&self, name: &str) -> Option<(&Key, &Node)> {
        self.0.iter().find(|(key, _)| key.name == name).map(|(key, node)| (key, node))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Key, &Node)> {
        self.0.iter().map(|(key, node)| (key, node))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Stack for the parser thread. Deserialization recurses once per nesting level, and an
/// unoptimised build needs about 45 KiB per level: [`Limits::max_depth`] levels must fit
/// whatever stack the caller runs on.
const PARSER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Parses one YAML document within `limits`. An empty document is a null scalar.
///
/// The parse runs on a short-lived thread with a stack sized for the deepest document
/// `limits` allow, so a hostile file can be rejected by its budget instead of overflowing
/// the caller's stack.
pub fn parse_document(source: &str, limits: &Limits) -> Result<Node, WorkflowError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("rbit-workflow-yaml".to_owned())
            .stack_size(PARSER_STACK_BYTES)
            .spawn_scoped(scope, || parse_document_here(source, limits))
            .map_err(|_| WorkflowError::new(ErrorKind::Parse, "the YAML parser could not be started"))?
            .join()
            .map_err(|_| WorkflowError::new(ErrorKind::Parse, "the YAML parser failed"))?
    })
}

fn parse_document_here(source: &str, limits: &Limits) -> Result<Node, WorkflowError> {
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_documents: 1,
            max_depth: limits.max_depth,
            max_nodes: limits.max_nodes,
            max_events: limits.max_nodes.saturating_mul(4),
            max_aliases: limits.max_aliases,
            max_anchors: limits.max_aliases,
            max_total_scalar_bytes: limits.max_bytes,
        },
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: limits.max_nodes,
            max_replay_stack_depth: limits.max_depth,
        },
        // Only `true` and `false` are booleans, as in YAML 1.2 and GitHub's parser; `on`
        // and `yes` stay strings (and are re-emitted as written).
        strict_booleans: true,
        non_finite_float_policy: serde_saphyr::options::NonFiniteFloatPolicy::PassThrough,
        with_snippet: false,
    };
    let mut node: Node = serde_saphyr::from_str_with_options(source, options).map_err(|error| {
        let at = error.location().map(|location| Location::new(location.line(), location.column()));
        let message = error.without_snippet().to_string();
        // serde-saphyr appends " at line L, column C"; the location is carried separately.
        let message = message.split(" at line ").next().unwrap_or(&message).to_owned();
        WorkflowError::new(ErrorKind::Parse, message).at_location(at)
    })?;
    recover_plain_text(&mut node, source);
    Ok(node)
}

/// Fills [`Scalar::plain`] from the source: a scalar written plain, on one line, untagged.
/// The span covers only the scalar's text, so quotes are seen in it, while a block scalar
/// header (`|-`) or a tag (`!!str`) is recognised in front of it.
fn recover_plain_text(node: &mut Node, source: &str) {
    match &mut node.value {
        Value::Scalar(scalar) => {
            scalar.plain = scalar
                .span
                .and_then(|(start, len)| Some((start, source.get(start..start.checked_add(len)?)?)))
                .filter(|&(start, text)| is_plain_spelling(text, &scalar.value) && !has_header(&source[..start]))
                .map(|(_, text)| text.to_owned());
        }
        Value::Sequence(items) => items.iter_mut().for_each(|item| recover_plain_text(item, source)),
        Value::Mapping(mapping) => mapping.0.iter_mut().for_each(|(_, value)| recover_plain_text(value, source)),
    }
}

fn is_plain_spelling(text: &str, value: &ScalarValue) -> bool {
    if text.is_empty() || text.contains(['\n', '\r']) || text.starts_with(['"', '\'', '|', '>', '!', '&', '*']) {
        return false;
    }
    match value {
        ScalarValue::String(string) => string == text,
        _ => true,
    }
}

/// Whether the source in front of a scalar ends with a block scalar header (`|`, `>-`,
/// `|2+`) or a tag (`!!str`, `!custom`), skipping whitespace.
fn has_header(before: &str) -> bool {
    let token = before.trim_end().rsplit(char::is_whitespace).next().unwrap_or("");
    token.starts_with('!') || token.trim_end_matches(|c: char| c == '-' || c == '+' || c.is_ascii_digit()).ends_with(['|', '>'])
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spanned = Spanned::<Value>::deserialize(deserializer)?;
        let at = Location::new(spanned.referenced.line(), spanned.referenced.column());
        let mut value = spanned.value;
        if let Value::Scalar(scalar) = &mut value {
            let span = spanned.defined.span();
            scalar.span = span
                .byte_offset()
                .zip(span.byte_len())
                .and_then(|(start, len)| Some((usize::try_from(start).ok()?, usize::try_from(len).ok()?)));
        }
        Ok(Self { value, at })
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl ValueVisitor {
    fn scalar(value: ScalarValue) -> Value {
        Value::Scalar(Scalar::new(value))
    }
}

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a YAML value")
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Null))
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Null))
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Int(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Int(value.into())))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Int(value)))
    }

    #[allow(clippy::cast_precision_loss, reason = "beyond i128, only the magnitude of a number is meaningful")]
    fn visit_u128<E>(self, value: u128) -> Result<Value, E> {
        Ok(Self::scalar(i128::try_from(value).map_or(ScalarValue::Float(value as f64), ScalarValue::Int)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::Float(value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Self::scalar(ScalarValue::String(value)))
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Value, E> {
        std::str::from_utf8(value)
            .map(|text| Self::scalar(ScalarValue::String(text.to_owned())))
            .map_err(|_| E::custom("binary data is not supported"))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(64));
        while let Some(item) = seq.next_element::<Node>()? {
            items.push(item);
        }
        Ok(Value::Sequence(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0).min(64));
        while let Some(key) = map.next_key::<Spanned<String>>()? {
            let at = Location::new(key.referenced.line(), key.referenced.column());
            entries.push((Key { name: key.value, at }, map.next_value::<Node>()?));
        }
        Ok(Value::Mapping(Mapping(entries)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Node {
        parse_document(source, &Limits::default()).unwrap()
    }

    fn scalars(node: &Node) -> Vec<(ScalarValue, Option<String>)> {
        node.as_sequence().unwrap().iter().map(|item| item.as_scalar().map(|s| (s.value.clone(), s.plain.clone())).unwrap()).collect()
    }

    #[test]
    fn scalars_keep_their_value_and_plain_spelling() {
        let doc =
            parse("- on\n- yes\n- true\n- 3.10\n- 0x1F\n- ~\n- \"3.10\"\n- 'it''s'\n- plain text\n- !!str 3.10\n- |-\n  2.0\n- .nan\n-\n");
        let plain = |text: &str| Some(text.to_owned());
        let got = scalars(&doc);
        assert_eq!(got[0], (ScalarValue::String("on".into()), plain("on")), "plain, so a runner reads it as it would have");
        assert_eq!(got[1], (ScalarValue::String("yes".into()), plain("yes")));
        assert_eq!(got[2], (ScalarValue::Bool(true), plain("true")));
        assert_eq!(got[3], (ScalarValue::Float(3.1), plain("3.10")));
        assert_eq!(got[4], (ScalarValue::Int(31), plain("0x1F")));
        assert_eq!(got[5], (ScalarValue::Null, plain("~")));
        assert_eq!(got[6], (ScalarValue::String("3.10".into()), None));
        assert_eq!(got[7], (ScalarValue::String("it's".into()), None));
        assert_eq!(got[8], (ScalarValue::String("plain text".into()), plain("plain text")));
        assert_eq!(got[9], (ScalarValue::String("3.10".into()), None), "a tagged string is not plain");
        assert_eq!(got[10], (ScalarValue::String("2.0".into()), None), "a block scalar is not plain");
        assert!(matches!(got[11], (ScalarValue::Float(value), _) if value.is_nan()));
        assert_eq!(got[12].0, ScalarValue::Null);
        let texts: Vec<_> = doc.as_sequence().unwrap().iter().map(|item| item.as_scalar().unwrap().text()).collect();
        assert_eq!(texts[..10], ["on", "yes", "true", "3.10", "0x1F", "~", "3.10", "it's", "plain text", "3.10"]);
    }

    #[test]
    fn mappings_keep_order_locations_and_resolve_aliases() {
        let doc = parse("b: &x {k: 1}\na: *x\n'on': [x]\nm:\n  <<: *x\n  j: 2\n");
        let mapping = doc.as_mapping().unwrap();
        let keys: Vec<_> = mapping.iter().map(|(key, _)| key.name.as_str()).collect();
        assert_eq!(keys, ["b", "a", "on", "m"]);
        assert_eq!(mapping.entry("a").unwrap().0.at, Location::new(2, 1));
        assert_eq!(mapping.get("a").unwrap().at.line, 2, "an alias is located where it is used");
        assert!(mapping.get("a").unwrap().same_value(mapping.get("b").unwrap()));
        let merged = mapping.get("m").unwrap().as_mapping().unwrap();
        assert_eq!(merged.get("k").and_then(Node::as_text).as_deref(), Some("1"));
        assert_eq!(merged.get("j").and_then(Node::as_text).as_deref(), Some("2"));
    }

    #[test]
    fn same_value_compares_data_like_go() {
        let doc = parse("- 1.10\n- 1.1\n- 1\n- 1.0\n- '1'\n- {a: [1]}\n- {a: [1]}\n- {a: [2]}\n");
        let items = doc.as_sequence().unwrap();
        assert!(items[0].same_value(&items[1]));
        assert!(!items[2].same_value(&items[3]));
        assert!(!items[2].same_value(&items[4]));
        assert!(items[5].same_value(&items[6]));
        assert!(!items[5].same_value(&items[7]));
    }

    #[test]
    fn errors_carry_their_location() {
        let error = parse_document("a: [1, 2\nb: 3", &Limits::default()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Parse);
        assert_eq!(error.at, Some(Location::new(2, 2)));
        assert!(!error.message.contains(" at line "), "{}", error.message);
        let duplicate = parse_document("k: 1\nk: 2\n", &Limits::default()).unwrap_err();
        assert!(duplicate.message.contains("duplicate"), "{}", duplicate.message);
        assert!(parse_document("a: 1\n---\nb: 2\n", &Limits::default()).is_err(), "one document only");
        assert!(parse_document("[a]: b\n", &Limits::default()).is_err(), "keys are strings");
        assert!(parse("").is_null());
    }

    #[test]
    fn budgets_stop_hostile_documents() {
        let limits = Limits::default();
        let deep = format!("{}1{}", "[".repeat(limits.max_depth + 1), "]".repeat(limits.max_depth + 1));
        assert!(parse_document(&deep, &limits).is_err());
        // Eleven levels of ten aliases each: 10^11 nodes if expanded.
        let laughs = (1..12u8)
            .map(|level| {
                let (previous, name) = (char::from(b'a' + level - 1), char::from(b'a' + level));
                format!("{name}: &{name} [{}]\n", vec![format!("*{previous}"); 10].join(", "))
            })
            .collect::<Vec<_>>()
            .concat();
        let laughs = format!("a: &a [x, x, x, x, x, x, x, x, x, x]\n{laughs}");
        assert!(parse_document(&laughs, &limits).is_err(), "billion laughs");
    }

    #[test]
    fn headers_in_front_of_a_scalar() {
        for before in ["a: |", "a: |-\n  ", "a: >+2\n", "- !!str ", "x: !custom  "] {
            assert!(has_header(before), "{before}");
        }
        for before in ["a: ", "- ", "[x, ", "a:\n  ", "", "a: &anchor ", "b: c>d, "] {
            assert!(!has_header(before), "{before}");
        }
        let doc = parse("a: |\n  yes\nb: !!str yes\nc:\n  yes\n");
        let mapping = doc.as_mapping().unwrap();
        let plain: Vec<_> = ["a", "b", "c"].iter().map(|key| mapping.get(key).unwrap().as_scalar().unwrap().plain.clone()).collect();
        assert_eq!(plain, [None, None, Some("yes".to_owned())]);
    }
}
