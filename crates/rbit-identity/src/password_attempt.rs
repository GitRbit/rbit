//! A consuming crypto operation retains its reservation through real worker completion.

use rbit_kernel::BlockingReservation;
use std::future::Future;
use zeroize::{ZeroizeOnDrop, Zeroizing};

use crate::{PasswordError, credential};

/// One admitted password operation; neither cloneable nor reusable.
///
/// It stores only capacity, never a password or stored PHC value. Each consuming
/// method submits one blocking worker. Cancelling its waiter after submission leaves
/// the worker's permit held until completion; cancelling before submission frees it.
/// Inputs enter zeroizing ownership synchronously, including for unpolled futures.
#[derive(Debug)]
pub struct PasswordAttempt {
    reservation: BlockingReservation,
}

impl PasswordAttempt {
    pub(crate) fn new(reservation: BlockingReservation) -> Self {
        Self { reservation }
    }

    /// Hashes with the existing Argon2id parameters and a fresh random salt.
    ///
    /// # Errors
    ///
    /// Returns the existing hash or Tokio join error. Admission already succeeded.
    pub fn hash(self, password: String) -> impl Future<Output = Result<String, PasswordError>> + Send {
        self.execute_protected(Zeroizing::new(password), |password| credential::hash(&password))
    }

    /// Verifies a stored PHC value. Wrong passwords and malformed hashes return false.
    ///
    /// # Errors
    ///
    /// Returns the existing Tokio join error; malformed PHC input is not an error.
    pub fn verify(self, password: String, phc_hash: String) -> impl Future<Output = Result<bool, PasswordError>> + Send {
        self.execute_protected((Zeroizing::new(password), Zeroizing::new(phc_hash)), |(password, phc_hash)| {
            Ok(credential::verify(&password, &phc_hash))
        })
    }

    /// Verifies login credentials, covering absent or unusable hashes with dummy work.
    ///
    /// A real supported mismatch returns false without a second dummy verification.
    /// Missing, malformed and unsupported credentials perform the existing dummy work
    /// and then reject, all under this one reservation. Inputs are protected before
    /// future construction, including when the returned future is never polled.
    /// The caller owns input bounds, account lookup, throttling and authorization.
    ///
    /// # Errors
    ///
    /// Returns actual crypto or worker errors, never disguising them as wrong passwords.
    pub fn verify_login(
        self,
        password: String,
        stored_phc: Option<String>,
    ) -> impl std::future::Future<Output = Result<bool, PasswordError>> + Send {
        let password = Zeroizing::new(password);
        let stored_phc = stored_phc.map(Zeroizing::new);
        async move { self.execute(move || credential::verify_login(&password, stored_phc.as_ref().map(|hash| hash.as_str()))).await }
    }

    /// Performs missing-login dummy work without discarding worker failures.
    ///
    /// The caller still rejects the missing login. This operation grants no identity
    /// and does not interpret its dummy hash's verification result as authentication.
    ///
    /// # Errors
    ///
    /// Returns the existing Tokio join error rather than disguising it as bad input.
    pub fn verify_dummy(self, password: String) -> impl Future<Output = Result<(), PasswordError>> + Send {
        self.execute_protected(Zeroizing::new(password), |password| {
            credential::verify_dummy(&password);
            Ok(())
        })
    }

    /// Verifies the current password, then hashes its replacement inside the same slot.
    ///
    /// Returns None for invalid current credentials without hashing the replacement.
    /// The caller owns validation, conditional storage updates and session revocation.
    /// No second admission or nested blocking task occurs between the two operations.
    ///
    /// # Errors
    ///
    /// Returns the existing replacement-hash or Tokio join error.
    pub fn verify_and_hash(
        self,
        current_password: String,
        current_phc: String,
        replacement: String,
    ) -> impl Future<Output = Result<Option<String>, PasswordError>> + Send {
        self.execute_protected(
            (Zeroizing::new(current_password), Zeroizing::new(current_phc), Zeroizing::new(replacement)),
            |(current_password, current_phc, replacement)| {
                let valid = credential::verify(&current_password, &current_phc);
                drop(current_password);
                drop(current_phc);
                if !valid {
                    return Ok(None);
                }
                credential::hash(&replacement).map(Some)
            },
        )
    }

    // The public, non-async entry points construct the wrappers before calling here.
    // Both this unpolled future and its eventual blocking closure own protected inputs.
    async fn execute_protected<S, F, T>(self, secrets: S, operation: F) -> Result<T, PasswordError>
    where
        S: ZeroizeOnDrop + Send + 'static,
        F: FnOnce(S) -> Result<T, PasswordError> + Send + 'static,
        T: Send + 'static,
    {
        self.execute(move || operation(secrets)).await
    }

    /// Hashes a candidate only if it differs from the password represented by `previous_phc`.
    ///
    /// Returns None when verification succeeds, allowing a caller to reject password
    /// reuse. A malformed previous hash fails verification and permits a new hash,
    /// preserving existing verification semantics. Both steps use this one admitted
    /// worker, even when the budget closes after the attempt was reserved.
    /// Authorization, input validation and persisted credential updates belong to callers.
    ///
    /// # Errors
    ///
    /// Returns the existing candidate-hash or Tokio join error.
    pub fn hash_if_different(
        self,
        password: String,
        previous_phc: String,
    ) -> impl std::future::Future<Output = Result<Option<String>, PasswordError>> + Send {
        let password = Zeroizing::new(password);
        let previous_phc = Zeroizing::new(previous_phc);
        async move {
            self.execute(move || {
                if credential::verify(&password, &previous_phc) {
                    return Ok(None);
                }
                drop(previous_phc);
                credential::hash(&password).map(Some)
            })
            .await
        }
    }

    async fn execute<F, T>(self, operation: F) -> Result<T, PasswordError>
    where
        F: FnOnce() -> Result<T, PasswordError> + Send + 'static,
        T: Send + 'static,
    {
        self.reservation.spawn(operation).await?
    }
}

#[cfg(test)]
mod secret_lifetime_tests;

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, sync::mpsc, time::Duration};

    use rbit_kernel::{AdmissionError, BlockingBudget};
    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn worker_failure_is_returned_and_releases_capacity() {
        let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
        let work = crate::PasswordWork::new(budget.clone());
        let result = work.begin().unwrap().execute::<_, ()>(|| panic!("simulated crypto worker failure")).await;
        assert!(matches!(result, Err(PasswordError::Join(error)) if error.is_panic()));
        assert_eq!(budget.available(), 1);
    }

    #[tokio::test]
    async fn cancelled_waiter_cannot_release_a_running_attempt() {
        let budget = BlockingBudget::new(NonZeroUsize::new(1).unwrap());
        let work = crate::PasswordWork::new(budget.clone());
        let attempt = work.begin().unwrap();
        let (started, ready) = oneshot::channel();
        let (release, proceed) = mpsc::channel();
        let request = tokio::spawn(attempt.execute(move || {
            started.send(()).unwrap();
            proceed.recv_timeout(Duration::from_secs(10)).unwrap();
            Ok(())
        }));
        ready.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        assert_eq!(work.begin().unwrap_err(), AdmissionError::Busy);
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while budget.available() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
