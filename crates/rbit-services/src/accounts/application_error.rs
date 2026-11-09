//! Protocol/application mapping stays in the legacy infrastructure adapter.

use rbit_identity::accounts::AccountError;
use rbit_storage::StorageError;

use crate::ServiceError;

pub fn registration(error: AccountError<StorageError>) -> ServiceError {
    map(error, |never| match never {})
}

pub(super) fn authentication(error: AccountError<StorageError, ServiceError>) -> ServiceError {
    map(error, std::convert::identity)
}

fn map<E>(error: AccountError<StorageError, E>, before: impl FnOnce(E) -> ServiceError) -> ServiceError {
    match error {
        AccountError::Store(error) => error.into(),
        AccountError::BeforeVerify(error) => before(error),
        AccountError::Name(error) => error.into(),
        AccountError::Invalid(message) => ServiceError::Invalid(message),
        AccountError::RegistrationClosed => ServiceError::RegistrationClosed,
        AccountError::SetupTokenRequired => ServiceError::Invalid(rbit_identity::accounts::SETUP_TOKEN_REQUIRED.into()),
        AccountError::InvalidCredentials => ServiceError::InvalidCredentials,
        AccountError::Suspended(name) => {
            tracing::info!(user = %name, "suspended user refused sign-in");
            ServiceError::Forbidden
        }
        AccountError::Admission(error) => error.into(),
        AccountError::Password(error) => error.into(),
        AccountError::Entropy(error) => ServiceError::internal(std::io::Error::other(error.to_string())),
    }
}
