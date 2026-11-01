//! Declared, bounded query projection. Values contain no raw URI or headers.
use crate::PageError;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
pub const MAX_QUERY_FIELDS: usize = 16;
pub const MAX_QUERY_VALUE_BYTES: usize = 2048;
pub const MAX_QUERY_BYTES: usize = 8192;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PagePresentation {
    #[default]
    Shell,
    Document,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteQuery(Vec<String>);
impl RouteQuery {
    pub fn new(keys: &[&str]) -> Result<Self, PageError> {
        let mut seen = BTreeSet::new();
        if keys.len() > MAX_QUERY_FIELDS || keys.iter().any(|key| !valid_key(key) || !seen.insert(*key)) {
            return Err(PageError::Invalid("query declarations"));
        }
        Ok(Self(keys.iter().map(|key| (*key).into()).collect()))
    }
    pub fn allows(&self, key: &str) -> bool {
        self.0.iter().any(|allowed| allowed == key)
    }
    pub fn keys(&self) -> &[String] {
        &self.0
    }
    pub(crate) fn validate(&self, query: &PageQuery) -> Result<(), PageError> {
        if query.0.keys().any(|key| !self.allows(key)) {
            return Err(PageError::Invalid("undeclared query field"));
        }
        Ok(())
    }
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PageQuery(BTreeMap<String, String>);
impl PageQuery {
    pub fn new(pairs: Vec<(String, String)>) -> Result<Self, PageError> {
        if pairs.len() > MAX_QUERY_FIELDS {
            return Err(PageError::Invalid("query field count"));
        }
        let mut map = BTreeMap::new();
        let mut total = 0usize;
        for (key, value) in pairs {
            total = total.checked_add(key.len()).and_then(|n| n.checked_add(value.len())).ok_or(PageError::Invalid("query size"))?;
            if !valid_key(&key)
                || value.len() > MAX_QUERY_VALUE_BYTES
                || total > MAX_QUERY_BYTES
                || value.chars().any(char::is_control)
                || map.insert(key, value).is_some()
            {
                return Err(PageError::Invalid("query fields or size"));
            }
        }
        Ok(Self(map))
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        self.0.iter().map(|(key, value)| (key.clone(), value.clone())).collect()
    }
    pub fn encoded(&self) -> String {
        self.0.iter().map(|(key, value)| format!("{}={}", encode(key), encode(value))).collect::<Vec<_>>().join("&")
    }
}
impl fmt::Debug for PageQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageQuery").field("fields", &self.0.len()).finish_non_exhaustive()
    }
}
fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    output
}
