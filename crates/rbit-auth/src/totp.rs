//! TOTP and recovery-code compatibility path; the algorithms, formats and their tests are
//! identity-owned (`rbit_identity::two_factor::totp`).

pub use rbit_identity::two_factor::totp::{DIGITS, PERIOD_SECONDS, RecoveryCode, TotpSecret, time_step};

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_paths_are_the_identity_types() {
        use std::any::TypeId;
        assert_eq!(TypeId::of::<super::TotpSecret>(), TypeId::of::<rbit_identity::two_factor::totp::TotpSecret>());
        assert_eq!(TypeId::of::<crate::RecoveryCode>(), TypeId::of::<rbit_identity::two_factor::totp::RecoveryCode>());
    }
}
