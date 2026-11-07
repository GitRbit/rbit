//! Persisted store-pair identifier, not a credential or startup-readiness receipt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SessionPairIdentity([u8; 16]);
impl SessionPairIdentity {
    pub fn from_storage(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.try_into().ok()?))
    }
    pub fn storage_bytes(self) -> [u8; 16] {
        self.0
    }
}
impl std::fmt::Debug for SessionPairIdentity {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SessionPairIdentity").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_is_bounded_exact_data_without_debug_bytes() {
        let bytes = [b'S'; 16];
        let identity = SessionPairIdentity::from_storage(&bytes).unwrap();
        assert_eq!(identity.storage_bytes(), bytes);
        for length in [0, 15, 17, 1000] {
            assert!(SessionPairIdentity::from_storage(&vec![0; length]).is_none());
        }
        assert!(!format!("{identity:?}").contains("SSSS"));
    }
}
