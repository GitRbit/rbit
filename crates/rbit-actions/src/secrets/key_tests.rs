//! Sealing and key file tests (moved from `rbit-services`, scopes written as their stored
//! addresses) and the stored formats that must not change.
use std::{
    future::{Future, ready},
    io,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use zeroize::Zeroizing;

use super::*;

fn key() -> SecretsKey {
    SecretsKey::from_bytes([7; 32])
}

const fn at(scope_kind: &str, scope_id: i64, secret_id: i64) -> SecretAddress<'_> {
    SecretAddress { scope_kind, scope_id, secret_id }
}

#[test]
fn a_sealed_value_opens_only_for_its_own_scope_and_id() {
    let key = key();
    let (nonce, ciphertext) = key.seal(at("repository", 4, 9), "hunter2").unwrap();
    assert_eq!(ciphertext.len(), "hunter2".len() + 16);
    assert_eq!(key.open(at("repository", 4, 9), key.id(), nonce, &ciphertext).unwrap().as_str(), "hunter2");
    assert!(key.open(at("repository", 4, 10), key.id(), nonce, &ciphertext).is_none());
    assert!(key.open(at("owner", 4, 9), key.id(), nonce, &ciphertext).is_none());
    assert!(key.open(at("repository", 5, 9), key.id(), nonce, &ciphertext).is_none());
}

#[test]
fn another_key_or_a_changed_ciphertext_does_not_open() {
    let (nonce, ciphertext) = key().seal(at("owner", 1, 1), "value").unwrap();
    let other = SecretsKey::from_bytes([8; 32]);
    assert!(!other.sealed(key().id()) && key().sealed(key().id()));
    assert!(other.open(at("owner", 1, 1), key().id(), nonce, &ciphertext).is_none());
    // A forged key id does not help either: the tag fails.
    assert!(other.open(at("owner", 1, 1), other.id(), nonce, &ciphertext).is_none());
    let mut tampered = ciphertext;
    tampered[0] ^= 1;
    assert!(key().open(at("owner", 1, 1), key().id(), nonce, &tampered).is_none());
}

#[test]
fn nonces_are_fresh_for_every_seal() {
    let key = key();
    let first = key.seal(at("repository", 1, 1), "same").unwrap();
    let second = key.seal(at("repository", 1, 1), "same").unwrap();
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
}

#[test]
fn the_stored_formats_are_unchanged() {
    // Values sealed before the move must still open: these bytes are stored.
    let aad = key::associated_data(at("environment", 12, 345));
    assert_eq!(aad, [b"rbit-actions-secret-v1\0environment\0".as_slice(), b"12\0", b"345"].concat());
    let text = key().encode();
    assert!(text.starts_with(KEY_PREFIX) && text.ends_with('\n'));
    assert_eq!(SecretsKey::parse(&text).unwrap().id(), key().id());
    assert_eq!(SecretsKey::parse(&format!("  {}  ", text.trim())).unwrap().id(), key().id());
    assert!(SecretsKey::parse("rbit-actions-key-v2:AAAA").unwrap_err().contains("does not start with"));
    assert!(SecretsKey::parse("rbit-actions-key-v1:%%%").unwrap_err().contains("base64"));
    assert!(SecretsKey::parse("rbit-actions-key-v1:AAAA").unwrap_err().contains("32 bytes"));
    assert!(!format!("{:?}", key()).contains("7, 7"));
}

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

/// An in-memory key file: its contents, whether others may read it, whether reads fail.
#[derive(Default)]
struct File {
    contents: Mutex<Option<String>>,
    shared: bool,
    broken: bool,
}

impl KeyFile for File {
    fn shown(&self) -> String {
        "/data/actions/secrets.key".to_owned()
    }

    fn read(&self) -> impl Future<Output = io::Result<Option<Zeroizing<String>>>> + Send {
        let result =
            if self.broken { Err(io::Error::other("disk on fire")) } else { Ok(self.contents.lock().unwrap().clone().map(Zeroizing::new)) };
        ready(result)
    }

    fn check_private(&self) -> Result<(), KeyUnavailable> {
        if self.shared { Err(KeyUnavailable("run `chmod 600` on it".to_owned())) } else { Ok(()) }
    }

    fn create(&self, contents: &str) -> impl Future<Output = io::Result<()>> + Send {
        *self.contents.lock().unwrap() = Some(contents.to_owned());
        ready(Ok(()))
    }
}

fn load(file: &File, sealed_exist: bool) -> Result<LoadedKey, KeyUnavailable> {
    run(load_key(file, ready(Ok::<_, &str>(sealed_exist))))
}

#[test]
fn a_missing_key_is_created_once_and_then_reused() {
    let file = File::default();
    let LoadedKey::Created(created) = load(&file, false).unwrap() else { panic!("created") };
    let LoadedKey::Existing(loaded) = load(&file, false).unwrap() else { panic!("reused") };
    assert_eq!(created.id(), loaded.id());
}

#[test]
fn a_missing_key_is_never_created_while_secrets_are_stored() {
    let file = File::default();
    let problem = load(&file, true).unwrap_err();
    assert!(problem.0.contains("missing while secrets are stored"), "{problem}");
    assert!(file.contents.lock().unwrap().is_none());
    let failed = run(load_key(&file, ready(Err::<bool, _>("database down")))).unwrap_err();
    assert_eq!(failed.0, "checking for stored secrets failed: database down");
}

#[test]
fn broken_shared_or_unreadable_key_files_are_refused_with_the_reason() {
    let file = File { contents: Mutex::new(Some("not a key".to_owned())), ..File::default() };
    assert!(load(&file, false).unwrap_err().0.starts_with("the secrets key file /data/actions/secrets.key is unusable: "));
    assert_eq!(file.contents.lock().unwrap().as_deref(), Some("not a key"), "kept");
    let shared = File { contents: Mutex::new(Some(key().encode().to_string())), shared: true, broken: false };
    assert!(load(&shared, false).unwrap_err().0.contains("chmod 600"));
    let broken = File { broken: true, ..File::default() };
    assert_eq!(load(&broken, false).unwrap_err().0, "reading the secrets key file /data/actions/secrets.key failed: disk on fire");
}
