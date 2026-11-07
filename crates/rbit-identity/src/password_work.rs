//! Injected password-work capacity; configuration and protocol policy belong to callers.

use rbit_kernel::{AdmissionError, BlockingBudget};

use crate::PasswordAttempt;

/// Shared admission for password hashing, verification and missing-login dummy work.
///
/// Composition creates one budget and passes clones to every password consumer. This
/// handle stores no credentials and chooses no numeric limit or retry/HTTP policy.
#[derive(Debug, Clone)]
pub struct PasswordWork {
    budget: BlockingBudget,
}

impl PasswordWork {
    /// Uses explicitly supplied capacity; clones retain the same kernel budget.
    pub fn new(budget: BlockingBudget) -> Self {
        Self { budget }
    }

    /// Admits one attempt before a caller consumes any one-use proof or mutates state.
    ///
    /// No password or account identifier is needed for this decision. Dropping the
    /// attempt before worker submission returns its slot without performing crypto.
    ///
    /// # Errors
    ///
    /// Returns [`AdmissionError::Busy`] immediately when saturated or
    /// [`AdmissionError::Closed`] after shutdown, without queueing or executing work.
    pub fn begin(&self) -> Result<PasswordAttempt, AdmissionError> {
        self.budget.try_reserve().map(PasswordAttempt::new)
    }

    /// Configured maximum of reserved, queued and running password attempts.
    pub fn capacity(&self) -> std::num::NonZeroUsize {
        self.budget.capacity()
    }

    /// Snapshot of reserved, queued and running attempts, for shutdown diagnostics.
    /// This can change immediately and must never be used for admission decisions.
    pub fn admitted(&self) -> usize {
        self.capacity().get() - self.budget.available()
    }

    /// Stops admission across clones. Already-admitted attempts may still finish.
    ///
    /// The host owns worker drain/runtime shutdown; this does not interrupt Argon2.
    pub fn close(&self) {
        self.budget.close();
    }
}
