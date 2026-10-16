//! Time-based one-time passwords (TOTP, RFC 6238) and single-use recovery codes.
//!
//! Codes are the 6-digit, 30-second, HMAC-SHA1 flavour every authenticator app supports.
//! [`TotpSecret::verify`] accepts the current time step and one step either side (for clock
//! drift) and reports *which* step matched, so callers can refuse a step that was already
//! used: that is the replay protection.

use std::fmt::{self, Write as _};

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::secret::random_bytes;

/// Seconds per time step.
pub const PERIOD_SECONDS: u64 = 30;
/// Digits per code.
pub const DIGITS: u32 = 6;
/// Steps accepted either side of the current one.
const SKEW_STEPS: u64 = 1;
/// 160 bits, the HMAC-SHA1 block-friendly size RFC 4226 recommends.
const SECRET_LEN: usize = 20;

/// The shared secret between the server and the user's authenticator app.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret(Vec<u8>);

impl TotpSecret {
    /// Draws a fresh 160-bit secret from the operating system's CSPRNG.
    pub fn generate() -> Self {
        Self(random_bytes()[..SECRET_LEN].to_vec())
    }

    /// Restores a stored secret. `None` unless it is 16–64 bytes long.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        (16..=64).contains(&bytes.len()).then(|| Self(bytes.to_vec()))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// RFC 4648 base32 without padding, as authenticator apps expect for manual entry.
    pub fn to_base32(&self) -> String {
        base32(&self.0)
    }

    /// The `otpauth://` URI encoded in the enrolment QR code.
    pub fn provisioning_uri(&self, issuer: &str, account: &str) -> String {
        let issuer = uri_component(issuer);
        format!(
            "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits={DIGITS}&period={PERIOD_SECONDS}",
            account = uri_component(account),
            secret = self.to_base32(),
        )
    }

    /// The code for time step `step`.
    pub fn code_at_step(&self, step: u64) -> u32 {
        hotp(&self.0, step, DIGITS)
    }

    /// Checks a submitted code against the steps around `unix_seconds`. Returns the
    /// matching step, which the caller must store and never accept again.
    pub fn verify(&self, code: &str, unix_seconds: i64) -> Option<u64> {
        let code = normalize_code(code)?;
        let now = time_step(unix_seconds);
        let mut matched = None;
        // Check every candidate so timing does not reveal which step matched.
        for step in now.saturating_sub(SKEW_STEPS)..=now + SKEW_STEPS {
            let expected = format!("{:0width$}", self.code_at_step(step), width = DIGITS as usize);
            if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) {
                matched = matched.or(Some(step));
            }
        }
        matched
    }
}

/// Never print secrets, even in debug logs.
impl fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TotpSecret(<redacted>)")
    }
}

/// The time step containing `unix_seconds` (0 before the epoch).
pub fn time_step(unix_seconds: i64) -> u64 {
    u64::try_from(unix_seconds).unwrap_or(0) / PERIOD_SECONDS
}

/// Digits only, spaces and dashes ignored (apps often show `123 456`). `None` otherwise.
fn normalize_code(code: &str) -> Option<String> {
    let digits: String = code.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    (digits.len() == DIGITS as usize && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits)
}

/// HOTP (RFC 4226): HMAC-SHA1 of the big-endian counter, dynamically truncated.
fn hotp(key: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(&counter.to_be_bytes());
    let hash = mac.finalize().into_bytes();
    let offset = usize::from(hash[hash.len() - 1] & 0x0f);
    let binary = u32::from_be_bytes([hash[offset], hash[offset + 1], hash[offset + 2], hash[offset + 3]]) & 0x7fff_ffff;
    binary % 10_u32.pow(digits)
}

fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 0x1f) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize]));
    }
    out
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
fn uri_component(value: &str) -> String {
    value.bytes().fold(String::with_capacity(value.len()), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
        out
    })
}

// ---------------------------------------------------------------------- recovery codes

/// Characters of recovery codes: digits and lower-case letters without `i`, `l`, `o`, `u`.
const RECOVERY_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
/// 12 characters × 5 bits = 60 bits per code.
const RECOVERY_CODE_LEN: usize = 12;

/// A single-use code that stands in for the authenticator app. Shown once; only
/// [`RecoveryCode::digest`] is stored.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryCode(String);

impl RecoveryCode {
    pub fn generate() -> Self {
        let random = random_bytes();
        // 32 divides 256, so taking each byte modulo 32 is uniform.
        Self(random[..RECOVERY_CODE_LEN].iter().map(|&b| char::from(RECOVERY_ALPHABET[usize::from(b % 32)])).collect())
    }

    /// Accepts what a user types: any case, with or without dashes and spaces.
    pub fn parse(input: &str) -> Option<Self> {
        let code: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').flat_map(char::to_lowercase).collect();
        (code.len() == RECOVERY_CODE_LEN && code.bytes().all(|b| RECOVERY_ALPHABET.contains(&b))).then_some(Self(code))
    }

    /// `xxxx-xxxx-xxxx`, for display.
    pub fn formatted(&self) -> String {
        let (a, rest) = self.0.split_at(4);
        let (b, c) = rest.split_at(4);
        format!("{a}-{b}-{c}")
    }

    /// SHA-256 over a domain tag, the owner and the code: a stored digest is useless for
    /// any other account. (60 random bits make a slow hash unnecessary.)
    pub fn digest(&self, owner: i64) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"rustit-recovery-code\0");
        hasher.update(owner.to_le_bytes());
        hasher.update(self.0.as_bytes());
        hasher.finalize().into()
    }
}

impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 appendix B, SHA-1 column: (time, 8-digit TOTP).
    const RFC_6238_VECTORS: [(i64, u32); 6] = [
        (59, 94_287_082),
        (1_111_111_109, 7_081_804),
        (1_111_111_111, 14_050_471),
        (1_234_567_890, 89_005_924),
        (2_000_000_000, 69_279_037),
        (20_000_000_000, 65_353_130),
    ];

    fn rfc_secret() -> TotpSecret {
        TotpSecret::from_bytes(b"12345678901234567890").unwrap()
    }

    #[test]
    fn matches_rfc_6238_test_vectors() {
        let secret = rfc_secret();
        for (time, expected) in RFC_6238_VECTORS {
            assert_eq!(hotp(secret.as_bytes(), time_step(time), 8), expected, "8 digits at t={time}");
            assert_eq!(secret.code_at_step(time_step(time)), expected % 1_000_000, "6 digits at t={time}");
        }
    }

    #[test]
    fn matches_rfc_4226_hotp_vectors() {
        let expected = [755_224, 287_082, 359_152, 969_429, 338_314, 254_676, 287_922, 162_583, 399_871, 520_489];
        for (counter, code) in expected.into_iter().enumerate() {
            assert_eq!(hotp(b"12345678901234567890", counter as u64, 6), code);
        }
    }

    #[test]
    fn verify_accepts_adjacent_steps_and_reports_the_step() {
        let secret = rfc_secret();
        let time = 1_111_111_111;
        let step = time_step(time);
        let code = |step| format!("{:06}", secret.code_at_step(step));
        assert_eq!(secret.verify(&code(step), time), Some(step));
        assert_eq!(secret.verify(&code(step - 1), time), Some(step - 1));
        assert_eq!(secret.verify(&code(step + 1), time), Some(step + 1));
        assert_eq!(secret.verify(&code(step + 2), time), None);
        assert_eq!(secret.verify(&code(step - 2), time), None);
        // Formatting from authenticator apps is tolerated; anything else is not.
        let spaced = format!("{} {}", &code(step)[..3], &code(step)[3..]);
        assert_eq!(secret.verify(&spaced, time), Some(step));
        assert_eq!(secret.verify("12345", time), None);
        assert_eq!(secret.verify("abcdef", time), None);
        assert_eq!(secret.verify("", time), None);
    }

    #[test]
    fn leading_zeros_are_kept() {
        // 07081804 → 081804 at t=1111111109.
        assert_eq!(rfc_secret().verify("081804", 1_111_111_109), Some(time_step(1_111_111_109)));
    }

    #[test]
    fn base32_matches_rfc_4648() {
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(base32(b"fooba"), "MZXW6YTB");
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(rfc_secret().to_base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(TotpSecret::generate().to_base32().len(), 32);
    }

    #[test]
    fn provisioning_uri_is_escaped() {
        let uri = rfc_secret().provisioning_uri("My Forge", "alice@example.com");
        assert_eq!(
            uri,
            "otpauth://totp/My%20Forge:alice%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=My%20Forge\
             &algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn secrets_are_validated_and_redacted() {
        assert!(TotpSecret::from_bytes(&[0; 10]).is_none());
        assert!(TotpSecret::from_bytes(&[0; 65]).is_none());
        assert_eq!(format!("{:?}", rfc_secret()), "TotpSecret(<redacted>)");
        assert_ne!(TotpSecret::generate(), TotpSecret::generate());
    }

    #[test]
    fn recovery_codes_parse_leniently_and_digest_per_owner() {
        let code = RecoveryCode::generate();
        let shown = code.formatted();
        assert_eq!(shown.len(), RECOVERY_CODE_LEN + 2);
        assert_eq!(RecoveryCode::parse(&shown), Some(code.clone()));
        assert_eq!(RecoveryCode::parse(&shown.to_uppercase().replace('-', " ")), Some(code.clone()));
        assert_eq!(RecoveryCode::parse("too-short"), None);
        assert_eq!(RecoveryCode::parse("iiii-llll-oooo"), None);
        assert_ne!(code.digest(1), code.digest(2));
        assert_eq!(code.digest(1), RecoveryCode::parse(&shown).unwrap().digest(1));
        assert_eq!(format!("{code:?}"), "RecoveryCode(<redacted>)");
    }
}
