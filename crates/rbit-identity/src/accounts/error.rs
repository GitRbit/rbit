//! Typed feature outcomes retain original storage/callback errors for adapters.

use rbit_contracts::NameError;
use rbit_kernel::AdmissionError;

use crate::PasswordError;

#[derive(Debug)]
pub enum AccountError<Store, Before = std::convert::Infallible> {
    Store(Store),
    BeforeVerify(Before),
    Name(NameError),
    Invalid(String),
    RegistrationClosed,
    SetupTokenRequired,
    InvalidCredentials,
    Suspended(crate::AccountName),
    Admission(AdmissionError),
    Password(PasswordError),
    Entropy(getrandom::Error),
}
