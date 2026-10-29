//! rbit Actions runner credentials and registration tokens (`docs/design/actions.md` §12.6).
//!
//! * A [`RunnerCredential`] (`rbrn_` + 43 base64url characters) is what a runner presents
//!   as `Authorization: Bearer` on every call. It is shown once, at registration.
//! * A [`RegistrationSecret`] (`rbrg_` + 43 base64url characters) is what an administrator
//!   hands to `rbit-runner register`. It is shown once, when it is created.
//!
//! Both carry 256 random bits and are stored only as SHA-256 digests in a unique index, so
//! a lookup is by digest and never compares secrets byte by byte; [`RunnerCredential::matches`]
//! exists for the rare comparison against a digest already in hand and runs in constant
//! time. The prefixes let secret scanners and the server tell them from every other token
//! without a lookup.

use std::fmt;

use subtle::ConstantTimeEq;

use crate::secret::{self, SECRET_BYTES, TokenDigest};

macro_rules! bearer_secret {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone)]
        pub struct $name([u8; SECRET_BYTES]);

        impl $name {
            /// The prefix of every value.
            pub const PREFIX: &'static str = $prefix;

            pub fn generate() -> Self {
                Self(secret::random_bytes())
            }

            /// Parses a full value; `None` unless it is exactly the prefix and 43 base64url
            /// characters.
            pub fn parse(value: &str) -> Option<Self> {
                value.strip_prefix(Self::PREFIX).and_then(secret::decode).map(Self)
            }

            /// Cheap check used to route credentials: does this look like one at all?
            pub fn looks_like(value: &str) -> bool {
                value.starts_with(Self::PREFIX)
            }

            pub fn encode(&self) -> String {
                format!("{}{}", Self::PREFIX, secret::encode(&self.0))
            }

            /// The digest it is stored and looked up by.
            pub fn digest(&self) -> TokenDigest {
                secret::digest(&self.0)
            }

            /// Whether this is the secret of `digest`, in constant time.
            pub fn matches(&self, digest: &TokenDigest) -> bool {
                self.digest().ct_eq(digest).into()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

bearer_secret!(
    /// The credential a registered runner authenticates with.
    RunnerCredential,
    "rbrn_"
);

bearer_secret!(
    /// A runner registration token: exchanged once (or as often as it allows) for a
    /// [`RunnerCredential`].
    RegistrationSecret,
    "rbrg_"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_digests() {
        let credential = RunnerCredential::generate();
        let encoded = credential.encode();
        assert!(encoded.starts_with("rbrn_") && encoded.len() == 5 + 43);
        assert!(!rbit_core::actions::looks_like_task_token(&encoded));
        let parsed = RunnerCredential::parse(&encoded).unwrap();
        assert_eq!(parsed.digest(), credential.digest());
        assert!(parsed.matches(&credential.digest()));
        assert!(!RunnerCredential::generate().matches(&credential.digest()));

        let token = RegistrationSecret::generate();
        assert!(token.encode().starts_with("rbrg_"));
        assert_eq!(RegistrationSecret::parse(&token.encode()).unwrap().digest(), token.digest());
    }

    #[test]
    fn kinds_do_not_mix_and_malformed_values_fail() {
        let credential = RunnerCredential::generate().encode();
        assert!(RegistrationSecret::parse(&credential).is_none());
        assert!(!RegistrationSecret::looks_like(&credential));
        assert!(RunnerCredential::looks_like(&credential));
        assert!(RunnerCredential::parse("rbrn_short").is_none());
        assert!(RunnerCredential::parse(&format!("{credential}A")).is_none());
        assert!(RunnerCredential::parse(&credential.replace("rbrn_", "rbit_")).is_none());
        assert_eq!(format!("{:?}", RunnerCredential::generate()), "RunnerCredential(<redacted>)");
    }
}
