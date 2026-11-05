//! Freeze externally observable identity behavior while relocating its defining crate.

use std::{
    any::TypeId,
    collections::{BTreeSet, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use rbit_contracts::{AccountId, RepoId};

#[test]
fn identifiers_preserve_scalar_json_and_the_full_signed_integer_range() {
    for value in [i64::MIN, -1, 0, 1, i64::MAX] {
        let wire = value.to_string();
        assert_eq!(serde_json::to_string(&AccountId(value)).unwrap(), wire);
        assert_eq!(serde_json::to_string(&RepoId(value)).unwrap(), wire);
        assert_eq!(serde_json::from_str::<AccountId>(&wire).unwrap().0, value);
        assert_eq!(serde_json::from_str::<RepoId>(&wire).unwrap().0, value);
    }
    for invalid in ["null", "true", "1.5", "\"42\"", "{\"0\":42}", "[42]", "9223372036854775808", "-9223372036854775809"] {
        assert!(serde_json::from_str::<AccountId>(invalid).is_err(), "{invalid}");
        assert!(serde_json::from_str::<RepoId>(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn ordering_hashing_and_debug_remain_integer_newtype_compatible() {
    let values = [i64::MAX, 0, i64::MIN, -1, 0];
    let accounts: BTreeSet<_> = values.map(AccountId).into_iter().collect();
    let repositories: BTreeSet<_> = values.map(RepoId).into_iter().collect();
    assert_eq!(accounts.into_iter().map(|id| id.0).collect::<Vec<_>>(), [i64::MIN, -1, 0, i64::MAX]);
    assert_eq!(repositories.into_iter().map(|id| id.0).collect::<Vec<_>>(), [i64::MIN, -1, 0, i64::MAX]);
    assert_eq!(values.map(AccountId).into_iter().collect::<HashSet<_>>().len(), 4);
    assert_eq!(values.map(RepoId).into_iter().collect::<HashSet<_>>().len(), 4);
    for value in values {
        assert_eq!(hash(AccountId(value)), hash(value));
        assert_eq!(hash(RepoId(value)), hash(value));
    }
    assert_eq!(format!("{:?}", AccountId(42)), "AccountId(42)");
    assert_eq!(format!("{:?}", RepoId(42)), "RepoId(42)");
    assert_ne!(TypeId::of::<AccountId>(), TypeId::of::<RepoId>());
}

fn hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
