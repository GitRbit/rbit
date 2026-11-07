//! Checked persistence facts for the inactive credential-fencing protocol.
//! These freely constructible snapshots are NOT authentication or session-issuance receipts.
use crate::accounts::LoginRecord;

/// One-use persistence outcome for the inactive original-proof protocol, not a
/// receipt. Invalidated origins must not be counted as an incorrect second factor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialProofWrite {
    Claimed,
    Rejected,
    Invalidated,
}

/// SQLite's nonnegative signed-integer epoch domain; never wraps or accepts negatives.
/// No Serde implementation: this is a local persistence fact, not a wire credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CredentialEpoch(i64);
impl CredentialEpoch {
    pub const INITIAL: Self = Self(0);
    pub const fn new(value: i64) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> i64 {
        self.0
    }
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// Pending refers only to the account's CURRENT epoch. Older unresolved intent rows
/// remain recovery work; they must not indefinitely block a newer completed epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialFence {
    pub epoch: CredentialEpoch,
    pub pending: bool,
}
impl CredentialFence {
    pub const fn accepts(self, original: CredentialEpoch) -> bool {
        !self.pending && self.epoch.get() == original.get()
    }
}

/// Same-row PHC and fence facts before verification. Applications must immediately
/// protect the owned PHC, as with `LoginRecord`; `Debug` inherits its credential redaction.
/// A later fresh lookup must never rehabilitate proof verified against this snapshot.
#[derive(Debug)]
pub struct CredentialLoginSnapshot {
    pub login: LoginRecord,
    pub fence: CredentialFence,
}

/// Same-row credential snapshot port. Implementations must return the PHC and fence
/// from one statement/snapshot, never a separate later epoch lookup.
pub trait CredentialLoginRepository: Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    fn find_login_snapshot(
        &self,
        login: &str,
    ) -> impl std::future::Future<Output = Result<Option<CredentialLoginSnapshot>, Self::Error>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_epoch_domain_and_increment_never_wrap() {
        assert_eq!(CredentialEpoch::new(-1), None);
        assert_eq!(CredentialEpoch::new(i64::MIN), None);
        assert_eq!(CredentialEpoch::new(0), Some(CredentialEpoch::INITIAL));
        assert_eq!(CredentialEpoch::INITIAL.next().unwrap().get(), 1);
        assert_eq!(CredentialEpoch::new(i64::MAX - 1).unwrap().next().unwrap().get(), i64::MAX);
        assert_eq!(CredentialEpoch::new(i64::MAX).unwrap().next(), None);
    }

    #[test]
    fn pending_and_changed_epoch_refuse_original_snapshot_even_at_numeric_extremes() {
        for epoch in [0, 1, i64::MAX] {
            let epoch = CredentialEpoch::new(epoch).unwrap();
            assert!(CredentialFence { epoch, pending: false }.accepts(epoch));
            assert!(!CredentialFence { epoch, pending: true }.accepts(epoch));
            let other = if epoch == CredentialEpoch::INITIAL { epoch.next().unwrap() } else { CredentialEpoch::INITIAL };
            assert!(!CredentialFence { epoch, pending: false }.accepts(other));
        }
    }
}
