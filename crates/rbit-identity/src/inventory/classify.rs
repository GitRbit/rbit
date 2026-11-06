use super::{Params, PasswordHash};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    Argon2d,
    Argon2i,
    Argon2id,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Parameters {
    pub algorithm: Algorithm,
    /// Absent version remains absent; the pinned verifier defaults to version19.
    pub version: Option<u32>,
    /// Effective costs from the pinned parser, including its defaults if omitted.
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Parameters { parameters: Parameters, salt_bytes: u64, digest_bytes: u64 },
    Incomplete,
    Malformed,
    Unsupported,
    Oversized,
}
/// Uses the pinned PHC/Argon parameter parsers only: no Argon context, allocation
/// proportional to m/t/p, hashing, verification, or attacker-controlled error output.
pub fn classify(bytes: &[u8], maximum: usize) -> Classification {
    if bytes.len() > maximum.min(4096) {
        return Classification::Oversized;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Classification::Malformed;
    };
    let Ok(hash) = PasswordHash::new(text) else {
        return Classification::Malformed;
    };
    let algorithm = match hash.algorithm.as_str() {
        "argon2d" => Algorithm::Argon2d,
        "argon2i" => Algorithm::Argon2i,
        "argon2id" => Algorithm::Argon2id,
        _ => return Classification::Unsupported,
    };
    if hash.version.is_some_and(|version| !matches!(version, 16 | 19)) {
        return Classification::Unsupported;
    }
    let (Some(salt), Some(digest)) = (&hash.salt, &hash.hash) else {
        return Classification::Incomplete;
    };
    let Ok(params) = Params::try_from(&hash) else {
        return Classification::Malformed;
    };
    Classification::Parameters {
        parameters: Parameters {
            algorithm,
            version: hash.version,
            memory_kib: params.m_cost(),
            iterations: params.t_cost(),
            parallelism: params.p_cost(),
        },
        salt_bytes: salt.len() as u64,
        digest_bytes: digest.len() as u64,
    }
}
