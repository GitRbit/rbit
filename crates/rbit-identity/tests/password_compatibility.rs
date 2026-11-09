//! Fixed published vectors cover verification independently of rewritten hash generation.

use std::num::NonZeroUsize;

use rbit_identity::PasswordWork;
use rbit_kernel::BlockingBudget;

// RustCrypto argon2-v0.6.0 tests/kat.rs, reference_argon2id_v0x13_2_8_2.
// Published reference password "password", salt "somesalt", m=256 KiB, t=2, p=2.
// https://github.com/RustCrypto/password-hashes/blob/argon2-v0.6.0/argon2/tests/kat.rs
// These nondefault parameters are a compatibility fixture, never a new-hash policy.
const PUBLISHED_ARGON2ID: &str = "$argon2id$v=19$m=256,t=2,p=2$c29tZXNhbHQ$bQk8UB/VmZZF4Oo79iDXuL5/0ttZwg2f/5U52iv1cDc";

#[tokio::test]
async fn the_bounded_verifier_accepts_the_fixed_nondefault_reference_hash() {
    let work = PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()));
    assert!(work.begin().unwrap().verify("password".into(), PUBLISHED_ARGON2ID.into()).await.unwrap());
    assert!(!work.begin().unwrap().verify("sassword".into(), PUBLISHED_ARGON2ID.into()).await.unwrap());
}
