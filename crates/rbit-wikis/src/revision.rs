//! Full native Git object identity, without a Git dependency or revision-expression API.
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WikiObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiRevisionError;

impl fmt::Display for WikiRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wiki revision requires a nonzero full lowercase Git object ID")
    }
}
impl std::error::Error for WikiRevisionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ObjectBytes {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

/// A syntactically valid identity, not proof of a commit, reachable object or access.
/// Fixed-size storage accepts SHA-1/SHA-256 without allocating a second hex String.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WikiRevisionId(ObjectBytes);

impl WikiRevisionId {
    pub const fn format(self) -> WikiObjectFormat {
        match self.0 {
            ObjectBytes::Sha1(_) => WikiObjectFormat::Sha1,
            ObjectBytes::Sha256(_) => WikiObjectFormat::Sha256,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            ObjectBytes::Sha1(bytes) => bytes,
            ObjectBytes::Sha256(bytes) => bytes,
        }
    }
}

impl TryFrom<&[u8]> for WikiRevisionId {
    type Error = WikiRevisionError;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if !matches!(bytes.len(), 20 | 32) || bytes.iter().all(|byte| *byte == 0) {
            return Err(WikiRevisionError);
        }
        match bytes.len() {
            20 => Ok(Self(ObjectBytes::Sha1(bytes.try_into().map_err(|_| WikiRevisionError)?))),
            32 => Ok(Self(ObjectBytes::Sha256(bytes.try_into().map_err(|_| WikiRevisionError)?))),
            _ => Err(WikiRevisionError),
        }
    }
}

impl FromStr for WikiRevisionId {
    type Err = WikiRevisionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')) {
            return Err(WikiRevisionError);
        }
        let mut bytes = match value.len() {
            40 => ObjectBytes::Sha1([0; 20]),
            _ => ObjectBytes::Sha256([0; 32]),
        };
        let target: &mut [u8] = match &mut bytes {
            ObjectBytes::Sha1(bytes) => bytes,
            ObjectBytes::Sha256(bytes) => bytes,
        };
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            target[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
        }
        if target.iter().all(|byte| *byte == 0) {
            return Err(WikiRevisionError);
        }
        Ok(Self(bytes))
    }
}

fn nibble(byte: u8) -> u8 {
    if byte.is_ascii_digit() { byte - b'0' } else { byte - b'a' + 10 }
}

impl fmt::Display for WikiRevisionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.as_bytes() {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
