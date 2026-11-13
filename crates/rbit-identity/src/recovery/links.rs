//! Who receives a link, and redeeming verification links.

use std::collections::HashSet;

use super::{
    EmailTokenRecord, NewEmailToken, RESET_LIFETIME_SECONDS, RecoveryAccount, RecoveryError, RecoveryRejection, RecoveryRepository,
    RecoveryResult, RecoveryThrottle, TokenPurpose, VERIFY_LIFETIME_SECONDS, link_digest, recipient_key, require_admin,
};
use crate::{Email, SessionToken, User};
use rbit_contracts::{AccountId, Timestamp};

/// The outcome of a reset request. Callers answer every outcome identically, so the
/// response never reveals whether an account uses the address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetRequest {
    Send(RecoveryAccount),
    /// The address belongs to a suspended account; nothing is sent.
    Suspended,
    /// No account uses the address; nothing is sent.
    Unknown,
}

#[derive(Debug)]
pub struct RecoveryLinks<'a, R, T> {
    recovery: &'a R,
    throttle: &'a T,
    email_available: bool,
}

impl<'a, R: RecoveryRepository, T: RecoveryThrottle> RecoveryLinks<'a, R, T> {
    pub fn new(recovery: &'a R, throttle: &'a T, email_available: bool) -> Self {
        Self { recovery, throttle, email_available }
    }

    fn require_email(&self) -> RecoveryResult<(), R::Error, T::Error> {
        if self.email_available { Ok(()) } else { Err(RecoveryError::Rejected(RecoveryRejection::EmailUnavailable)) }
    }

    async fn account(&self, account: AccountId) -> RecoveryResult<RecoveryAccount, R::Error, T::Error> {
        self.recovery.find_account(account).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)
    }

    /// Throttles the client, then the recipient, before looking the address up.
    pub async fn request_reset(&self, address: &str, client_key: &str) -> RecoveryResult<ResetRequest, R::Error, T::Error> {
        self.require_email()?;
        self.throttle.check_client(client_key).map_err(RecoveryError::Throttle)?;
        let email: Email = address.trim().parse().map_err(|_| RecoveryError::Rejected(RecoveryRejection::InvalidAddress))?;
        self.throttle.check_recipient(&recipient_key(&email)).map_err(RecoveryError::Throttle)?;
        Ok(match self.recovery.find_account_by_email(&email).await.map_err(RecoveryError::Store)? {
            Some(account) if !account.suspended => ResetRequest::Send(account),
            Some(_) => ResetRequest::Suspended,
            None => ResetRequest::Unknown,
        })
    }

    /// The account a valid, unused reset link belongs to. [`RecoveryError::NotFound`] for
    /// unknown, expired or used links; [`RecoveryError::Forbidden`] if since suspended.
    pub async fn reset_account(&self, token: &str) -> RecoveryResult<RecoveryAccount, R::Error, T::Error> {
        reset_account(self.recovery, token).await
    }

    /// Stores a new link for the account's current address and returns its secret.
    pub async fn issue(&self, account: &RecoveryAccount, purpose: TokenPurpose) -> RecoveryResult<String, R::Error, T::Error> {
        let lifetime = match purpose {
            TokenPurpose::Reset => RESET_LIFETIME_SECONDS,
            TokenPurpose::Verify => VERIFY_LIFETIME_SECONDS,
        };
        let secret = SessionToken::generate();
        let token = NewEmailToken {
            digest: secret.digest(),
            account_id: account.id,
            purpose,
            email: account.email.clone(),
            expires_at: Timestamp(Timestamp::now().seconds() + lifetime),
        };
        self.recovery.insert(&token).await.map_err(RecoveryError::Store)?;
        Ok(secret.encode())
    }

    /// The verification link's record, without using it up: [`RecoveryError::NotFound`]
    /// for an unknown, expired or used link, or one sent to an address the account no
    /// longer uses.
    pub async fn verify_target(&self, token: &str) -> RecoveryResult<EmailTokenRecord, R::Error, T::Error> {
        let digest = link_digest(token).ok_or(RecoveryError::NotFound)?;
        let record = self
            .recovery
            .find_valid(digest, TokenPurpose::Verify, Timestamp::now())
            .await
            .map_err(RecoveryError::Store)?
            .ok_or(RecoveryError::NotFound)?;
        let account = self.recovery.find_account(record.account_id).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)?;
        if account.email != record.email {
            return Err(RecoveryError::NotFound);
        }
        Ok(record)
    }

    /// Uses up a verification link and confirms the address it was sent to, provided the
    /// account still uses it; then retires the account's other verification links.
    pub async fn verify_email(&self, token: &str) -> RecoveryResult<EmailTokenRecord, R::Error, T::Error> {
        let digest = link_digest(token).ok_or(RecoveryError::NotFound)?;
        let record = self
            .recovery
            .consume(digest, TokenPurpose::Verify, Timestamp::now())
            .await
            .map_err(RecoveryError::Store)?
            .ok_or(RecoveryError::NotFound)?;
        if !self.recovery.mark_verified(record.account_id, &record.email, Timestamp::now()).await.map_err(RecoveryError::Store)? {
            return Err(RecoveryError::NotFound);
        }
        self.recovery.revoke_all(record.account_id, TokenPurpose::Verify).await.map_err(RecoveryError::Store)?;
        Ok(record)
    }

    /// The signed-in user's account, when a new verification link may be sent to it.
    pub async fn resend_target(&self, user: &User) -> RecoveryResult<RecoveryAccount, R::Error, T::Error> {
        self.require_email()?;
        let account = self.account(user.id()).await?;
        if account.email_verified_at.is_some() {
            return Err(RecoveryError::Rejected(RecoveryRejection::AlreadyVerified));
        }
        self.throttle.check_recipient(&recipient_key(&account.email)).map_err(RecoveryError::Throttle)?;
        Ok(account)
    }

    /// The user's address and its verification state.
    pub async fn email_status(&self, user: &User) -> RecoveryResult<RecoveryAccount, R::Error, T::Error> {
        self.account(user.id()).await
    }

    /// Which of `ids` have a verified address; administrators only.
    pub async fn verified_among(&self, actor: &User, ids: &[AccountId]) -> RecoveryResult<HashSet<AccountId>, R::Error, T::Error> {
        require_admin::<R::Error, T::Error, std::convert::Infallible>(actor)?;
        self.recovery.verified_among(ids).await.map_err(RecoveryError::Store)
    }

    /// The account an administrator may send a reset link to.
    pub async fn admin_reset_target(&self, actor: &User, target: AccountId) -> RecoveryResult<RecoveryAccount, R::Error, T::Error> {
        require_admin::<R::Error, T::Error, std::convert::Infallible>(actor)?;
        self.require_email()?;
        let account = self.account(target).await?;
        if account.suspended {
            return Err(RecoveryError::Rejected(RecoveryRejection::Suspended(account.name)));
        }
        Ok(account)
    }

    /// A new account needs a verification link unless its address is already verified
    /// (for example by an external provider).
    pub async fn sign_up_verification(&self, account: AccountId) -> RecoveryResult<Option<RecoveryAccount>, R::Error, T::Error> {
        let account = self.recovery.find_account(account).await.map_err(RecoveryError::Store)?;
        Ok(account.filter(|account| account.email_verified_at.is_none()))
    }

    /// A changed address needs a verification link; re-entering the previous address in
    /// another letter case does not.
    pub async fn changed_email_verification(
        &self,
        account: AccountId,
        previous: &str,
    ) -> RecoveryResult<Option<RecoveryAccount>, R::Error, T::Error> {
        let account = self.recovery.find_account(account).await.map_err(RecoveryError::Store)?;
        Ok(account.filter(|account| !account.email.as_str().eq_ignore_ascii_case(previous)))
    }
}

/// Shared by reset preview and completion: the link must be honoured now and its account
/// must not be suspended.
pub(super) async fn reset_account<R: RecoveryRepository, T, V>(
    recovery: &R,
    token: &str,
) -> RecoveryResult<RecoveryAccount, R::Error, T, V> {
    let digest = link_digest(token).ok_or(RecoveryError::NotFound)?;
    let record = recovery
        .find_valid(digest, TokenPurpose::Reset, Timestamp::now())
        .await
        .map_err(RecoveryError::Store)?
        .ok_or(RecoveryError::NotFound)?;
    let account = recovery.find_account(record.account_id).await.map_err(RecoveryError::Store)?.ok_or(RecoveryError::NotFound)?;
    if account.suspended {
        return Err(RecoveryError::Forbidden);
    }
    Ok(account)
}
