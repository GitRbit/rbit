//! Narrow data-only host capabilities. Cookies, transport and identity proofs stay
//! with the host; callbacks cannot name response headers or cookie attributes.
use crate::{FormAction, MutationError, PageActor};
use rbit_messages::LocaleSnapshot;
use std::fmt;
use tokio_util::sync::CancellationToken;

pub const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CONCURRENT_UPLOADS: usize = 4;
pub const UPLOAD_FIELD: &str = "pack";
#[derive(Clone)]
pub struct UploadData(Vec<u8>);
impl UploadData {
    pub fn new(bytes: Vec<u8>) -> Result<Self, MutationError> {
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(MutationError::InvalidForm);
        }
        Ok(Self(bytes))
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}
impl fmt::Debug for UploadData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadData").field("bytes", &self.0.len()).finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleTag(String);
impl LocaleTag {
    pub fn new(value: &str) -> Result<Self, MutationError> {
        let mut parts = value.split('-');
        let Some(primary) = parts.next() else {
            return Err(MutationError::InvalidForm);
        };
        if value.len() > 64
            || primary.is_empty()
            || primary.len() > 8
            || !primary.bytes().all(|b| b.is_ascii_alphabetic())
            || parts.any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            return Err(MutationError::InvalidForm);
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleChoice {
    Automatic,
    Language(LocaleTag),
}
impl LocaleChoice {
    pub fn new(value: &str) -> Result<Self, MutationError> {
        if value.is_empty() { Ok(Self::Automatic) } else { Ok(Self::Language(LocaleTag::new(value)?)) }
    }
    pub fn preference(&self) -> Option<&str> {
        match self {
            Self::Automatic => None,
            Self::Language(tag) => Some(tag.as_str()),
        }
    }
}
#[derive(Debug, Clone)]
pub struct PreferenceContext {
    pub actor: Option<PageActor>,
    pub locale: LocaleSnapshot,
    pub form: FormAction,
    pub(crate) cancellation: CancellationToken,
}
impl PreferenceContext {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.child_token()
    }
}
