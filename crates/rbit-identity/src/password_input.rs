//! Bounded Unicode validation with the existing minimum-before-maximum semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordLength {
    TooShort,
    TooLong,
    Accepted,
}

/// Inspects only enough scalar values to distinguish the existing validation outcomes.
/// Does not truncate, normalize, allocate or impose a new password policy.
pub fn classify_password_length(password: &str, minimum: usize, maximum: usize) -> PasswordLength {
    let limit = minimum.max(maximum).saturating_add(1);
    let length = password.chars().take(limit).count();
    if length < minimum {
        PasswordLength::TooShort
    } else if length > maximum {
        PasswordLength::TooLong
    } else {
        PasswordLength::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn full_count_oracle(password: &str, minimum: usize, maximum: usize) -> PasswordLength {
        let count = password.chars().count();
        if count < minimum {
            PasswordLength::TooShort
        } else if count > maximum {
            PasswordLength::TooLong
        } else {
            PasswordLength::Accepted
        }
    }
    #[test]
    fn matches_independent_full_count_for_unicode_boundaries_and_pathological_policy() {
        for minimum in [0, 1, 10, 255, 256, 257, 1024, usize::MAX] {
            for maximum in [0, 1, 256, 257, usize::MAX] {
                for count in [0, 1, 9, 10, 255, 256, 257, 1023, 1024, 1025] {
                    for unit in ["a", "é", "界", "👩", "e\u{301}"] {
                        let input = unit.repeat(count);
                        assert_eq!(classify_password_length(&input, minimum, maximum), full_count_oracle(&input, minimum, maximum));
                    }
                }
            }
        }
    }
    #[test]
    fn long_input_does_not_change_minimum_error_precedence() {
        let password = "界".repeat(100_000);
        assert_eq!(classify_password_length(&password, 10, 256), PasswordLength::TooLong);
        assert_eq!(classify_password_length(&password, 100_001, 256), PasswordLength::TooShort);
        assert_eq!(classify_password_length(&password, 256, 256), PasswordLength::TooLong);
    }
}
