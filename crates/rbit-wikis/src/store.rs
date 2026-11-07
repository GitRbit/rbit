//! Persistent catalog/object-root binding, not a secret or authentication receipt.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiStoreIdError;

/// Fixed random catalog namespace. SQLite allocates it; the dedicated Git root must
/// contain the same marker. Copying a matched backup preserves it intentionally.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WikiStoreId([u8; 16]);

impl WikiStoreId {
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl TryFrom<[u8; 16]> for WikiStoreId {
    type Error = WikiStoreIdError;
    fn try_from(bytes: [u8; 16]) -> Result<Self, Self::Error> {
        if bytes == [0; 16] { Err(WikiStoreIdError) } else { Ok(Self(bytes)) }
    }
}

impl fmt::Debug for WikiStoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiStoreId(..)")
    }
}
